use mail_parser::{MessagePart, MimeHeaders};

pub fn determine_attachment_name(
    message_part: &MessagePart,
    attachment_depth: &usize,
    attachment_order: &usize,
) -> String {
    let attachment_name = message_part
        .attachment_name()
        .or_else(|| {
            message_part
                .content_disposition()
                .and_then(|cd| cd.attribute("filename"))
        })
        .or_else(|| {
            message_part
                .content_type()
                .and_then(|ct| ct.attribute("name"))
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("attachment-{}-{}", attachment_depth, attachment_order + 1));

    let mut attachment_name = sanitize_filename::sanitize(&attachment_name);

    if std::path::Path::new(&attachment_name).extension().is_none() {
        if let Some(ct) = message_part.content_type() {
            let mime = format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("octet-stream"));
            if let Some(exts) = mime_guess::get_mime_extensions_str(&mime) {
                attachment_name.push('.');
                attachment_name.push_str(exts[0]);
            }
        }
    }

    return attachment_name;
}
