mod attachment;

pub mod local;
pub mod s3;
use crate::smtp::models::Metadata;
use mail_parser::Message;

use async_trait::async_trait;

pub const NO_BODY_FALLBACK: &str = r#"
<html>
    <h3>Body not found</h3>
    <p>This message had no body when captured by smtp2s.</p>
</html>
"#;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, metadata: &Metadata, message: &Message<'_>) -> Result<(), std::io::Error>;
}
