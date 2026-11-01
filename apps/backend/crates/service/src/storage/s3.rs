//! S3 互換ストレージバックエンド（AWS S3 / MinIO 等）。

use std::env;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use object_store::aws::AmazonS3Builder;
use object_store::{
    Attribute, Attributes, ObjectStore, PutMultipartOpts, PutOptions, PutPayload, path::Path,
};

use super::r#trait::{ByteStream, StorageBackend, StorageError};

const MULTIPART_THRESHOLD: u64 = 5 * 1024 * 1024;
const MULTIPART_PART_SIZE: usize = 5 * 1024 * 1024;

/// S3 互換 API へオブジェクトを保存するバックエンド。
#[derive(Clone)]
pub struct S3StorageBackend {
    store: Arc<dyn ObjectStore>,
    bucket: String,
    #[allow(dead_code)]
    public_base_url: String,
}

impl std::fmt::Debug for S3StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3StorageBackend")
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

impl S3StorageBackend {
    /// 環境変数から設定を読み込んでインスタンスを生成する。
    ///
    /// 必須: `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`
    /// 任意: `S3_PUBLIC_BASE_URL`, `S3_FORCE_PATH_STYLE`（デフォルト false）
    pub async fn from_env() -> Result<Self, StorageError> {
        let endpoint = env::var("S3_ENDPOINT")
            .map_err(|_| StorageError::Other("S3_ENDPOINT is not set".into()))?;
        let bucket = env::var("S3_BUCKET")
            .map_err(|_| StorageError::Other("S3_BUCKET is not set".into()))?;
        let region = env::var("S3_REGION")
            .map_err(|_| StorageError::Other("S3_REGION is not set".into()))?;
        let access_key_id = env::var("S3_ACCESS_KEY_ID")
            .map_err(|_| StorageError::Other("S3_ACCESS_KEY_ID is not set".into()))?;
        let secret_access_key = env::var("S3_SECRET_ACCESS_KEY")
            .map_err(|_| StorageError::Other("S3_SECRET_ACCESS_KEY is not set".into()))?;

        let force_path_style = env::var("S3_FORCE_PATH_STYLE")
            .ok()
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let public_base_url = env::var("S3_PUBLIC_BASE_URL").unwrap_or_else(|_| {
            format!(
                "{}/{}",
                endpoint.trim_end_matches('/'),
                bucket.trim_start_matches('/')
            )
        });

        let mut builder = AmazonS3Builder::new()
            .with_endpoint(&endpoint)
            .with_bucket_name(&bucket)
            .with_region(&region)
            .with_access_key_id(&access_key_id)
            .with_secret_access_key(&secret_access_key)
            .with_allow_http(endpoint.starts_with("http://"));

        if force_path_style {
            builder = builder.with_virtual_hosted_style_request(false);
        }

        let store = builder
            .build()
            .map_err(|e| StorageError::Other(format!("S3 client build failed: {e}")))?;

        Ok(Self {
            store: Arc::new(store),
            bucket,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
        })
    }
}

fn to_path(key: &str) -> Result<Path, StorageError> {
    if key.is_empty() {
        return Err(StorageError::InvalidKey);
    }
    Path::parse(key).map_err(|_| StorageError::InvalidKey)
}

fn mime_attributes(mime: &str) -> Attributes {
    let mut attrs = Attributes::new();
    attrs.insert(Attribute::ContentType, mime.to_string().into());
    attrs
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn upload(
        &self,
        key: &str,
        mut stream: ByteStream,
        content_length: Option<u64>,
        mime: &str,
    ) -> Result<(), StorageError> {
        let path = to_path(key)?;

        // 単一 PUT は全量を貯めるので、小さいと分かっている本文に限る。長さ未知は multipart
        if let Some(expected) = content_length.filter(|&n| n < MULTIPART_THRESHOLD) {
            let mut buffer = Vec::with_capacity(expected as usize);
            let mut read: u64 = 0;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                read += chunk.len() as u64;
                buffer.extend_from_slice(&chunk);
            }
            if read != expected {
                return Err(StorageError::SizeMismatch {
                    expected,
                    actual: read,
                });
            }

            let opts = PutOptions {
                attributes: mime_attributes(mime),
                ..Default::default()
            };
            self.store
                .put_opts(&path, PutPayload::from(Bytes::from(buffer)), opts)
                .await
                .map_err(|e| StorageError::Other(format!("S3 PutObject failed: {e}")))?;
        } else {
            let opts = PutMultipartOpts {
                attributes: mime_attributes(mime),
                ..Default::default()
            };
            let mut upload = self
                .store
                .put_multipart_opts(&path, opts)
                .await
                .map_err(|e| {
                    StorageError::Other(format!("S3 CreateMultipartUpload failed: {e}"))
                })?;

            // chunk は pending の空き分だけ足し、満杯（5 MiB）になったら送る。
            // pending は MULTIPART_PART_SIZE を越えない
            let mut pending = BytesMut::with_capacity(MULTIPART_PART_SIZE);
            let mut uploaded: u64 = 0;

            let result: Result<(), StorageError> = async {
                while let Some(chunk) = stream.next().await {
                    let mut chunk = chunk?;
                    uploaded += chunk.len() as u64;
                    while !chunk.is_empty() {
                        let part = if pending.is_empty() && chunk.len() >= MULTIPART_PART_SIZE {
                            // part を丸ごと取れるなら複製せずスライスで送る
                            chunk.split_to(MULTIPART_PART_SIZE)
                        } else {
                            let room = MULTIPART_PART_SIZE - pending.len();
                            pending.extend_from_slice(&chunk.split_to(room.min(chunk.len())));
                            if pending.len() < MULTIPART_PART_SIZE {
                                break; // chunk を使い切った
                            }
                            std::mem::replace(
                                &mut pending,
                                BytesMut::with_capacity(MULTIPART_PART_SIZE),
                            )
                            .freeze()
                        };
                        upload.put_part(PutPayload::from(part)).await.map_err(|e| {
                            StorageError::Other(format!("S3 UploadPart failed: {e}"))
                        })?;
                    }
                }
                if !pending.is_empty() {
                    upload
                        .put_part(PutPayload::from(std::mem::take(&mut pending).freeze()))
                        .await
                        .map_err(|e| StorageError::Other(format!("S3 UploadPart failed: {e}")))?;
                }
                if let Some(expected) = content_length
                    && uploaded != expected
                {
                    return Err(StorageError::SizeMismatch {
                        expected,
                        actual: uploaded,
                    });
                }
                // complete の失敗も下の abort に回す（未完了の multipart を残さない）
                upload.complete().await.map_err(|e| {
                    StorageError::Other(format!("S3 CompleteMultipartUpload failed: {e}"))
                })?;
                Ok(())
            }
            .await;

            if result.is_err() {
                let _ = upload.abort().await;
                return result;
            }
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = to_path(key)?;
        self.store
            .delete(&path)
            .await
            .map_err(|e| StorageError::Other(format!("S3 DeleteObject failed: {e}")))?;
        Ok(())
    }

    async fn get_stream(&self, key: &str) -> Result<ByteStream, StorageError> {
        let path = to_path(key)?;
        let result = self.store.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::NotFound,
            e => StorageError::Other(format!("S3 GetObject failed: {e}")),
        })?;

        let stream = result.into_stream().map(|chunk| {
            chunk.map_err(|e| StorageError::Other(format!("S3 GetObject stream failed: {e}")))
        });

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // to_path: Path::parse は先頭スラッシュを正規化、バックスラッシュをパーセントエンコードするため
    // 拒否するのは空文字と ".." を含むパスのみ。

    #[test]
    fn to_path_rejects_empty() {
        assert!(matches!(to_path(""), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn to_path_rejects_double_dot() {
        assert!(matches!(
            to_path("../escape"),
            Err(StorageError::InvalidKey)
        ));
        assert!(matches!(
            to_path("foo/../bar"),
            Err(StorageError::InvalidKey)
        ));
        assert!(matches!(to_path(".."), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn to_path_accepts_valid_keys() {
        assert!(to_path("uuid-v4-key").is_ok());
        assert!(to_path("prefix/uuid-key").is_ok());
        assert!(to_path("a").is_ok());
        assert!(to_path("abc123-_.").is_ok());
    }

    /// upload / delete / get_stream が to_path 経由で不正な key を拒否することを確認する。
    /// InMemory ストアを使うことでネットワークアクセスなしにテストできる。
    fn dummy_backend() -> S3StorageBackend {
        S3StorageBackend {
            store: Arc::new(object_store::memory::InMemory::new()),
            bucket: "test".into(),
            public_base_url: "http://localhost:1/test".into(),
        }
    }

    #[test]
    fn upload_rejects_invalid_key() {
        let backend = dummy_backend();
        let stream: ByteStream = Box::pin(futures::stream::empty());
        let result = futures::executor::block_on(backend.upload("", stream, None, "text/plain"));
        assert!(matches!(result, Err(StorageError::InvalidKey)));
    }

    #[test]
    fn delete_rejects_invalid_key() {
        let backend = dummy_backend();
        let result = futures::executor::block_on(backend.delete("../escape"));
        assert!(matches!(result, Err(StorageError::InvalidKey)));
    }

    #[test]
    fn get_stream_rejects_invalid_key() {
        let backend = dummy_backend();
        let result = futures::executor::block_on(backend.get_stream(""));
        assert!(matches!(result, Err(StorageError::InvalidKey)));
    }

    use object_store::{
        GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, PutResult, UploadPart,
    };
    use std::sync::Mutex;

    /// InMemory に委譲しつつ、単一 PUT の回数と multipart の part の大きさ・abort の回数を記録する。
    /// `fail_complete` なら multipart の complete を失敗させる。
    #[derive(Debug, Default)]
    struct Recording {
        inner: object_store::memory::InMemory,
        puts: Mutex<usize>,
        parts: Arc<Mutex<Vec<usize>>>,
        aborts: Arc<Mutex<usize>>,
        fail_complete: bool,
    }

    impl std::fmt::Display for Recording {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Recording")
        }
    }

    #[derive(Debug)]
    struct RecordingUpload {
        inner: Box<dyn MultipartUpload>,
        parts: Arc<Mutex<Vec<usize>>>,
        aborts: Arc<Mutex<usize>>,
        fail_complete: bool,
    }

    #[async_trait]
    impl MultipartUpload for RecordingUpload {
        fn put_part(&mut self, data: PutPayload) -> UploadPart {
            self.parts.lock().unwrap().push(data.content_length());
            self.inner.put_part(data)
        }
        async fn complete(&mut self) -> object_store::Result<PutResult> {
            if self.fail_complete {
                return Err(object_store::Error::Generic {
                    store: "Recording",
                    source: "complete failed".into(),
                });
            }
            self.inner.complete().await
        }
        async fn abort(&mut self) -> object_store::Result<()> {
            *self.aborts.lock().unwrap() += 1;
            self.inner.abort().await
        }
    }

    #[async_trait]
    impl ObjectStore for Recording {
        async fn put_opts(
            &self,
            location: &Path,
            payload: PutPayload,
            opts: PutOptions,
        ) -> object_store::Result<PutResult> {
            *self.puts.lock().unwrap() += 1;
            self.inner.put_opts(location, payload, opts).await
        }
        async fn put_multipart_opts(
            &self,
            location: &Path,
            opts: PutMultipartOpts,
        ) -> object_store::Result<Box<dyn MultipartUpload>> {
            let inner = self.inner.put_multipart_opts(location, opts).await?;
            Ok(Box::new(RecordingUpload {
                inner,
                parts: self.parts.clone(),
                aborts: self.aborts.clone(),
                fail_complete: self.fail_complete,
            }))
        }
        async fn get_opts(
            &self,
            location: &Path,
            options: GetOptions,
        ) -> object_store::Result<GetResult> {
            self.inner.get_opts(location, options).await
        }
        async fn delete(&self, location: &Path) -> object_store::Result<()> {
            self.inner.delete(location).await
        }
        fn list(
            &self,
            prefix: Option<&Path>,
        ) -> futures::stream::BoxStream<'_, object_store::Result<ObjectMeta>> {
            self.inner.list(prefix)
        }
        async fn list_with_delimiter(
            &self,
            prefix: Option<&Path>,
        ) -> object_store::Result<ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }
        async fn copy(&self, from: &Path, to: &Path) -> object_store::Result<()> {
            self.inner.copy(from, to).await
        }
        async fn copy_if_not_exists(&self, from: &Path, to: &Path) -> object_store::Result<()> {
            self.inner.copy_if_not_exists(from, to).await
        }
    }

    const MIB: usize = 1024 * 1024;

    fn recording_backend(fail_complete: bool) -> (S3StorageBackend, Arc<Recording>) {
        let store = Arc::new(Recording {
            fail_complete,
            ..Default::default()
        });
        let backend = S3StorageBackend {
            store: store.clone(),
            bucket: "test".into(),
            public_base_url: "http://localhost:1/test".into(),
        };
        (backend, store)
    }

    /// 1 MiB ずつ `mib` 個流し、`fail` なら最後に Err を流す。
    fn mib_stream(mib: usize, fail: bool) -> ByteStream {
        let chunks = (0..mib).map(|i| Ok(Bytes::from(vec![i as u8; MIB])));
        let tail = fail.then(|| Err(StorageError::Other("client aborted".into())));
        Box::pin(futures::stream::iter(chunks.chain(tail)))
    }

    fn read_back(backend: &S3StorageBackend, key: &str) -> Result<Vec<u8>, StorageError> {
        futures::executor::block_on(async {
            let mut stream = backend.get_stream(key).await?;
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk?);
            }
            Ok(out)
        })
    }

    /// 長さ未知の本文は全量を貯めず、5 MiB ごとの part で流す。
    #[test]
    fn upload_unknown_length_streams_in_parts() {
        let (backend, store) = recording_backend(false);
        futures::executor::block_on(backend.upload("k", mib_stream(6, false), None, "a/b"))
            .expect("upload");

        assert_eq!(*store.puts.lock().unwrap(), 0, "単一 PUT を使わない");
        assert_eq!(*store.parts.lock().unwrap(), vec![5 * MIB, MIB]);
        let expected: Vec<u8> = (0..6).flat_map(|i| vec![i as u8; MIB]).collect();
        assert_eq!(read_back(&backend, "k").expect("read back"), expected);
    }

    /// 途中で Err が来たら multipart を abort し、オブジェクトを残さない。
    #[test]
    fn upload_unknown_length_error_saves_nothing() {
        let (backend, store) = recording_backend(false);
        let result =
            futures::executor::block_on(backend.upload("k", mib_stream(6, true), None, "a/b"));

        assert!(result.is_err());
        assert_eq!(
            *store.parts.lock().unwrap(),
            vec![5 * MIB],
            "part は送り始めていた"
        );
        assert_eq!(*store.aborts.lock().unwrap(), 1);
        assert!(matches!(
            read_back(&backend, "k"),
            Err(StorageError::NotFound)
        ));
    }

    /// 5 MiB 未満と分かっている本文は単一 PUT。
    #[test]
    fn upload_small_known_length_uses_single_put() {
        let (backend, store) = recording_backend(false);
        futures::executor::block_on(backend.upload(
            "k",
            mib_stream(1, false),
            Some(MIB as u64),
            "a/b",
        ))
        .expect("upload");

        assert_eq!(*store.puts.lock().unwrap(), 1);
        assert!(store.parts.lock().unwrap().is_empty());
        assert_eq!(read_back(&backend, "k").expect("read back").len(), MIB);
    }

    /// 不揃いな chunk でも part は 5 MiB ちょうど（最後だけ端数）に詰め、内容と順序を保つ。
    /// 5 MiB 以上の chunk は pending を通らずにスライスで送る経路も通す。
    #[test]
    fn upload_packs_uneven_chunks_into_full_parts() {
        let (backend, store) = recording_backend(false);
        let chunks: Vec<Bytes> = [3 * MIB, 7 * MIB, 1, 11 * MIB]
            .iter()
            .enumerate()
            .map(|(i, &n)| Bytes::from(vec![i as u8 + 1; n]))
            .collect();
        let expected = chunks.concat();
        let stream: ByteStream = Box::pin(futures::stream::iter(chunks.into_iter().map(Ok)));
        futures::executor::block_on(backend.upload("k", stream, None, "a/b")).expect("upload");

        assert_eq!(
            *store.parts.lock().unwrap(),
            vec![5 * MIB, 5 * MIB, 5 * MIB, 5 * MIB, MIB + 1]
        );
        assert_eq!(read_back(&backend, "k").expect("read back"), expected);
    }

    /// complete が失敗しても abort し、未完了の multipart を残さない。
    #[test]
    fn upload_aborts_when_complete_fails() {
        let (backend, store) = recording_backend(true);
        let result =
            futures::executor::block_on(backend.upload("k", mib_stream(6, false), None, "a/b"));

        assert!(result.is_err());
        assert_eq!(*store.aborts.lock().unwrap(), 1);
    }
}
