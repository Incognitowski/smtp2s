use std::collections::HashSet;

use async_recursion::async_recursion;
use async_trait::async_trait;
use aws_sdk_s3::{primitives::ByteStream, Client};
use mail_parser::Message;
use tracing::{error, info};
use ulid::Ulid;

use crate::{
    smtp::models::Metadata,
    storage::{attachment::determine_attachment_name, Storage},
};

use super::NO_BODY_FALLBACK;

pub struct S3FileStorage {
    client: Client,
    bucket_name: String,
}

impl S3FileStorage {
    pub fn new(client: Client, bucket: String) -> Self {
        Self {
            client,
            bucket_name: bucket,
        }
    }
}

#[async_trait]
impl Storage for S3FileStorage {
    async fn save(&self, metadata: &Metadata, message: &Message<'_>) -> Result<(), std::io::Error> {
        let execution_id = Ulid::new().to_string();

        // Upload metadata
        let metadata_key = format!("{}/metadata.json", &execution_id);
        let metadata_body = serde_json::to_vec_pretty(&metadata).unwrap();
        self.upload_object(&metadata_key, metadata_body).await;

        // Upload message body
        let message_key = format!("{}/body.html", &execution_id);
        let message_body = message
            .body_html(0)
            .unwrap_or(std::borrow::Cow::Owned(NO_BODY_FALLBACK.to_string()))
            .into_owned()
            .into_bytes();
        self.upload_object(&message_key, message_body).await;

        // Upload attachments
        let mut file_names: HashSet<String> = HashSet::new();
        self.save_attachments_from_message(message, &execution_id, 0, &mut file_names)
            .await?;

        Ok(())
    }
}

impl S3FileStorage {
    async fn upload_object(&self, key: &str, body: Vec<u8>) {
        info!("About to upload {} to bucket {}", key, self.bucket_name);

        let put_request = self
            .client
            .put_object()
            .bucket(self.bucket_name.clone())
            .key(key)
            .body(ByteStream::from(body))
            .send()
            .await;

        match put_request {
            Ok(_) => info!("{} uploaded successfully", key),
            Err(err) => error!(
                "Failed to upload {}, error is {:?}",
                key,
                err.into_service_error()
            ),
        }
    }

    #[async_recursion]
    async fn save_attachments_from_message(
        &self,
        msg: &mail_parser::Message<'_>,
        execution_id: &str,
        depth: usize,
        file_names: &mut HashSet<String>,
    ) -> Result<(), std::io::Error> {
        for (i, part) in msg.attachments().enumerate() {
            let original_name = determine_attachment_name(part, &depth, &i);
            let mut name = original_name.clone();

            while file_names.contains(&name) {
                let ulid = &Ulid::new().to_string()[20..];
                match name.rsplit_once('.') {
                    Some((file_base_name, ext)) => {
                        name = format!("{}-{}.{}", file_base_name, ulid, ext)
                    }
                    None => name = format!("{}-{}", name, ulid),
                }
            }

            file_names.insert(name.clone());

            self.upload_object(
                &format!("{}/attachments/{}", execution_id, name),
                part.contents().to_vec(),
            )
            .await;

            if let Some(nested) = part.message() {
                self.save_attachments_from_message(nested, execution_id, depth + 1, file_names)
                    .await?;
            }
        }
        Ok(())
    }
}
