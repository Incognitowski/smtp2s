use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use mail_parser::MimeHeaders;

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

        // Save metadata
        let mut metadata_output_file =
            File::create(base_folder.join(format!("{}-metadata.json", &execution)))?;
        metadata_output_file
            .write_all(serde_json::to_string_pretty(&metadata).unwrap().as_bytes())?;

        // Save body
        if let Some(body) = message.body_html(0) {
            let mut body_output_file =
                File::create(base_folder.join(format!("{}-body.html", &execution)))?;
            body_output_file.write_all(body.as_bytes())?;
        } else if let Some(body) = message.body_text(0) {
            let mut body_output_file =
                File::create(base_folder.join(format!("{}-body.txt", &execution)))?;
            body_output_file.write_all(body.as_bytes())?;
        }

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
        let mut name = part
            .attachment_name()
            .or_else(|| part.content_disposition().and_then(|cd| cd.attribute("filename")))
            .or_else(|| part.content_type().and_then(|ct| ct.attribute("name")))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("attachment-{}-{}", depth, i + 1));

        name = sanitize_filename::sanitize(&name);

        if std::path::Path::new(&name).extension().is_none() {
            if let Some(ct) = part.content_type() {
                let mime = format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("octet-stream"));
                if let Some(exts) = mime_guess::get_mime_extensions_str(&mime) {
                    name.push('.');
                    name.push_str(exts[0]);
                }
            }
        }

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
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("attachment");
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
