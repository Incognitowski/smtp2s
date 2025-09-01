pub mod local;
use crate::smtp::models::Metadata;
use mail_parser::Message;

pub trait Storage {
    fn save(&self, metadata: &Metadata, message: &Message) -> Result<(), std::io::Error>;
}
