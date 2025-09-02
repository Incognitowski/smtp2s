use crate::storage::attachment::determine_attachment_name;
use async_recursion::async_recursion;
use async_trait::async_trait;
use mail_parser::Message;
use std::path::{Path, PathBuf};
use tokio::fs;
use ulid::Ulid;

use crate::smtp::models::Metadata;
use crate::storage::Storage;
use super::NO_BODY_FALLBACK;

pub struct LocalFileStorage {
    pub base_path: PathBuf,
}

#[async_trait]
impl Storage for LocalFileStorage {
    async fn save(&self, metadata: &Metadata, message: &Message<'_>) -> Result<(), std::io::Error> {
        let execution = Ulid::new().to_string();
        let base_folder = &self.base_path;
        fs::create_dir_all(base_folder).await?;

        // Save metadata file
        fs::write(
            base_folder.join(format!("{}-metadata.json", &execution)),
            serde_json::to_string_pretty(&metadata).unwrap().as_bytes(),
        )
        .await?;

        // Save message body file
        fs::write(
            base_folder.join(format!("{}-body.html", &execution)),
            message
                .body_html(0)
                .unwrap_or(std::borrow::Cow::Owned(NO_BODY_FALLBACK.to_string()))
                .as_bytes(),
        )
        .await?;

        // Save attachments
        save_attachments_from_message(&message, &base_folder, 0).await?;

        Ok(())
    }
}

#[async_recursion]
async fn save_attachments_from_message(
    msg: &mail_parser::Message<'_>,
    out_dir: &Path,
    depth: usize,
) -> Result<(), std::io::Error> {
    for (i, part) in msg.attachments().enumerate() {
        let name = determine_attachment_name(part, &depth, &i);

        let path = dedupe_filename(out_dir.join(&name)).await;
        fs::write(&path, part.contents()).await?;

        if let Some(nested) = part.message() {
            save_attachments_from_message(nested, out_dir, depth + 1).await?;
        }
    }
    Ok(())
}

async fn dedupe_filename(path: PathBuf) -> PathBuf {
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return path;
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for n in 2.. {
        let candidate = if ext.is_empty() {
            path.with_file_name(format!("{stem} ({n})"))
        } else {
            path.with_file_name(format!("{stem} ({n}).{ext}"))
        };
        if !tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
            return candidate;
        }
    }
    unreachable!()
}