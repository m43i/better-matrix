//! Actions on an attachment, independent of the timeline or preview that shows it.

use matrix_core::{AccountId, Attachment, AttachmentKind, Command, MediaSize};

use super::{Purpose, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentAction {
    CopyImage,
    Download,
    Open,
}

impl State {
    pub fn act_on_attachment(&mut self, account: AccountId, file: &Attachment, action: AttachmentAction) {
        self.popover = None;
        let Some(media) = file.media.clone() else { return };
        match action {
            AttachmentAction::CopyImage if file.kind == AttachmentKind::Image => {
                self.dispatch(Command::FetchMedia { account, media, size: MediaSize::Original }, Purpose::CopyImage);
            }
            AttachmentAction::CopyImage => {}
            AttachmentAction::Download => self.download(account, media, &file.name),
            AttachmentAction::Open => {
                let path = self.shell.attachment_path(&file.name);
                self.dispatch(Command::SaveMedia { account, media, path: path.clone() }, Purpose::OpenFile(path));
            }
        }
    }
}

#[cfg(test)]
mod tests;
