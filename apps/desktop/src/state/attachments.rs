//! Actions on an attachment, independent of the timeline or preview that shows it.

use matrix_core::{AccountId, Attachment, AttachmentKind, Command, MediaSize};

use super::{Purpose, State};

/// Documents, media and archives the system may view; programs and unknown formats are downloaded instead.
pub fn can_open_attachment(file: &Attachment) -> bool {
    if file.media.is_none() {
        return false;
    }
    let name = file.name.trim_end_matches(['.', ' ']);
    let Some(extension) = std::path::Path::new(name).extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "pdf"
            | "txt"
            | "md"
            | "rtf"
            | "csv"
            | "tsv"
            | "doc"
            | "docx"
            | "odt"
            | "xls"
            | "xlsx"
            | "ods"
            | "ppt"
            | "pptx"
            | "odp"
            | "pages"
            | "numbers"
            | "key"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "ico"
            | "svg"
            | "avif"
            | "heic"
            | "heif"
            | "tif"
            | "tiff"
            | "mp4"
            | "m4v"
            | "mov"
            | "mkv"
            | "webm"
            | "avi"
            | "mpg"
            | "mpeg"
            | "mp3"
            | "m4a"
            | "aac"
            | "ogg"
            | "oga"
            | "opus"
            | "wav"
            | "flac"
            | "zip"
            | "7z"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentAction {
    CopyImage,
    Download,
    Open,
}

impl State {
    /// Acts on a file from its message in `room_id`: nothing while its media is held back there
    /// (ADR 0011), as the menu doesn't offer it then.
    pub fn act_on_message_attachment(
        &mut self,
        account: &AccountId,
        room_id: &str,
        event_id: &str,
        file: &Attachment,
        action: AttachmentAction,
    ) {
        let held = self.accounts.get(account).is_some_and(|data| {
            data.hides_listed_media(room_id, event_id, file, &self.revealed_media)
                || data.ignores_sender_of(room_id, event_id)
        });
        if held {
            self.popover = None;
            return;
        }
        self.act_on_attachment(account.clone(), file, action);
    }

    pub fn act_on_attachment(&mut self, account: AccountId, file: &Attachment, action: AttachmentAction) {
        self.popover = None;
        // What the preview holds back can't be saved, copied or opened either (ADR 0011).
        // Nor what someone the user ignores sent.
        if (self.preview_held() || self.preview_item_ignored())
            && self.preview.as_ref().is_some_and(|preview| preview.attachment == *file)
        {
            return;
        }
        let Some(media) = file.media.clone() else { return };
        match action {
            AttachmentAction::CopyImage if file.kind == AttachmentKind::Image => {
                self.dispatch(Command::FetchMedia { account, media, size: MediaSize::Original }, Purpose::CopyImage);
            }
            AttachmentAction::CopyImage => {}
            AttachmentAction::Download => self.download(account, media, &file.name),
            AttachmentAction::Open if can_open_attachment(file) => {
                let path = self.shell.attachment_path(&file.name);
                self.dispatch(Command::SaveMedia { account, media, path: path.clone() }, Purpose::OpenFile(path));
            }
            AttachmentAction::Open => {}
        }
    }
}

#[cfg(test)]
mod tests;
