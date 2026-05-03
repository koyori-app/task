
use backend::server::run;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = &sea_orm::Database::connect("postgresql://postgres:umkmnosnjfgoklsf@db.catarks.org:5432/task").await?;
    db.get_schema_registry("backend::entities::*").sync(db).await?;
    run().await;

    Ok(())
}
