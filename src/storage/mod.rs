mod attachment;

pub mod local;
pub mod s3;
use crate::smtp::models::Metadata;
use mail_parser::Message;

pub trait Storage {
    fn save(&self, metadata: &Metadata, message: &Message) -> Result<(), std::io::Error>;
}
