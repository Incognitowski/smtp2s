use crate::storage::attachment::determine_attachment_name;
use std::fs;
use std::path::{Path, PathBuf};

use mail_parser::Message;
use ulid::Ulid;

use crate::smtp::models::Metadata;
use crate::storage::Storage;

pub struct LocalFileStorage {
    pub base_path: PathBuf,
}

impl Storage for LocalFileStorage {
    fn save(&self, metadata: &Metadata, message: &Message) -> Result<(), std::io::Error> {
        let execution = Ulid::new().to_string();
        let base_folder = &self.base_path;
        fs::create_dir_all(base_folder)?;

        // Save metadata file
        fs::write(
            base_folder.join(format!("{}-metadata.json", &execution)),
            serde_json::to_string_pretty(&metadata).unwrap().as_bytes(),
        )?;

        // Save message body file
        fs::write(
            base_folder.join(format!("{}-body.html", &execution)),
            message
                .body_html(0)
                .unwrap_or(std::borrow::Cow::Owned(NO_BODY_FALLBACK.to_string()))
                .as_bytes(),
        )?;

        // Save attachments
        save_attachments_from_message(&message, &base_folder, 0)?;

        Ok(())
    }
}

fn save_attachments_from_message(
    msg: &mail_parser::Message,
    out_dir: &Path,
    depth: usize,
) -> Result<(), std::io::Error> {
    for (i, part) in msg.attachments().enumerate() {
        let name = determine_attachment_name(part, &depth, &i);

        let path = dedupe_filename(out_dir.join(&name));
        fs::write(&path, part.contents())?;

        if let Some(nested) = part.message() {
            save_attachments_from_message(nested, out_dir, depth + 1)?;
        }
    }
    Ok(())
}

fn dedupe_filename(path: PathBuf) -> PathBuf {
    if !path.exists() {
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
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

const NO_BODY_FALLBACK: &str = r#"
<html>
    <h3>Body not found</h3>
    <p>This message had no body when captured by smtp2s.</p>
</html>
"#;
