//! S3 互換ストレージバックエンド（AWS S3 / MinIO 等）。

use std::env;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Builder as S3ConfigBuilder, Region};
use aws_sdk_s3::primitives::ByteStream as AwsByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use bytes::Bytes;
use futures::StreamExt;

use super::r#trait::{ByteStream, StorageBackend, StorageError};

const MULTIPART_THRESHOLD: u64 = 5 * 1024 * 1024;
const MULTIPART_PART_SIZE: usize = 5 * 1024 * 1024;

/// S3 互換 API へオブジェクトを保存するバックエンド。
#[derive(Clone, Debug)]
pub struct S3StorageBackend {
    client: Client,
    bucket: String,
    public_base_url: String,
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

        let credentials = Credentials::new(access_key_id, secret_access_key, None, None, "env");

        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .region(Region::new(region))
            .load()
            .await;

        let s3_config = S3ConfigBuilder::from(&shared_config)
            .endpoint_url(endpoint)
            .force_path_style(force_path_style)
            .build();

        Ok(Self {
            client: Client::from_conf(s3_config),
            bucket,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
        })
    }
}

fn validate_key(key: &str) -> Result<(), StorageError> {
    if key.is_empty() || key.starts_with('/') || key.contains("..") || key.contains('\\') {
        return Err(StorageError::InvalidKey);
    }
    Ok(())
}

async fn read_stream_to_bytes(
    mut stream: ByteStream,
    content_length: u64,
) -> Result<Bytes, StorageError> {
    let mut buffer = Vec::with_capacity(content_length.min(MULTIPART_THRESHOLD) as usize);
    let mut read: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        read += chunk.len() as u64;
        buffer.extend_from_slice(&chunk);
    }

    if content_length > 0 && read != content_length {
        return Err(StorageError::SizeMismatch {
            expected: content_length,
            actual: read,
        });
    }

    Ok(Bytes::from(buffer))
}

async fn put_object(
    client: &Client,
    bucket: &str,
    key: &str,
    mime: &str,
    body: Bytes,
) -> Result<(), StorageError> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type(mime)
        .body(AwsByteStream::from(body))
        .send()
        .await
        .map_err(|e| StorageError::Other(format!("S3 PutObject failed: {e}")))?;
    Ok(())
}

async fn multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    mime: &str,
    mut stream: ByteStream,
    content_length: u64,
) -> Result<(), StorageError> {
    let create = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .content_type(mime)
        .send()
        .await
        .map_err(|e| StorageError::Other(format!("S3 CreateMultipartUpload failed: {e}")))?;

    let upload_id = create
        .upload_id()
        .ok_or_else(|| StorageError::Other("S3 upload_id missing".into()))?
        .to_string();

    let result =
        multipart_upload_inner(client, bucket, key, &upload_id, &mut stream, content_length).await;

    if result.is_err() {
        let _ = client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(&upload_id)
            .send()
            .await;
    }

    result
}

async fn multipart_upload_inner(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    stream: &mut ByteStream,
    content_length: u64,
) -> Result<(), StorageError> {
    let mut part_number: i32 = 1;
    let mut completed_parts = Vec::new();
    let mut pending = Vec::with_capacity(MULTIPART_PART_SIZE);
    let mut uploaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        uploaded += chunk.len() as u64;
        pending.extend_from_slice(&chunk);

        while pending.len() >= MULTIPART_PART_SIZE {
            let part_bytes = Bytes::from(pending.drain(..MULTIPART_PART_SIZE).collect::<Vec<_>>());
            let etag = upload_part(client, bucket, key, upload_id, part_number, part_bytes).await?;
            completed_parts.push(
                CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(etag)
                    .build(),
            );
            part_number += 1;
        }
    }

    if !pending.is_empty() {
        let etag = upload_part(
            client,
            bucket,
            key,
            upload_id,
            part_number,
            Bytes::from(pending),
        )
        .await?;
        completed_parts.push(
            CompletedPart::builder()
                .part_number(part_number)
                .e_tag(etag)
                .build(),
        );
    }

    if content_length > 0 && uploaded != content_length {
        return Err(StorageError::SizeMismatch {
            expected: content_length,
            actual: uploaded,
        });
    }

    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(completed_parts))
        .build();

    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .map_err(|e| StorageError::Other(format!("S3 CompleteMultipartUpload failed: {e}")))?;

    Ok(())
}

async fn upload_part(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: i32,
    body: Bytes,
) -> Result<String, StorageError> {
    let output = client
        .upload_part()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(AwsByteStream::from(body))
        .send()
        .await
        .map_err(|e| StorageError::Other(format!("S3 UploadPart failed: {e}")))?;

    output
        .e_tag()
        .map(str::to_string)
        .ok_or_else(|| StorageError::Other("S3 UploadPart e_tag missing".into()))
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn upload(
        &self,
        key: &str,
        stream: ByteStream,
        content_length: u64,
        mime: &str,
    ) -> Result<(), StorageError> {
        validate_key(key)?;

        if content_length < MULTIPART_THRESHOLD {
            let body = read_stream_to_bytes(stream, content_length).await?;
            put_object(&self.client, &self.bucket, key, mime, body).await
        } else {
            multipart_upload(
                &self.client,
                &self.bucket,
                key,
                mime,
                stream,
                content_length,
            )
            .await
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        validate_key(key)?;

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Other(format!("S3 DeleteObject failed: {e}")))?;

        Ok(())
    }

    async fn get_stream(&self, key: &str) -> Result<ByteStream, StorageError> {
        validate_key(key)?;

        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Other(format!("S3 GetObject failed: {e}")))?;

        let body = output.body;
        let stream = futures::stream::unfold(body, |mut body| async move {
            match body.try_next().await {
                Ok(Some(chunk)) => Some((Ok(Bytes::from(chunk)), body)),
                Ok(None) => None,
                Err(e) => Some((
                    Err(StorageError::Other(format!(
                        "S3 GetObject stream failed: {e}"
                    ))),
                    body,
                )),
            }
        });

        Ok(Box::pin(stream))
    }
}
