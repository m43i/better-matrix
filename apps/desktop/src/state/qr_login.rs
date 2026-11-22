//! A QR sign-in under way (ADR 0003), on either device: the new one, from the sign-in card, or
//! the signed-in one, linking a new device from the Sessions page. Both follow the engine's
//! [`QrLoginStep`]s; the check code the user types goes back as an answer.

use matrix_core::{QrLoginStep, RequestId};
use rune::TextEdit;

/// Where a QR sign-in stands, as the dialog or card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct QrLogin {
    /// The command running it; `None` once it failed, until the user tries again.
    pub request: Option<RequestId>,
    pub step: QrLoginStep,
    /// The check code being typed, at [`QrLoginStep::EnterCheckCode`].
    pub code: TextEdit,
    /// The check code went back: waiting for the next step.
    pub answered: bool,
    pub error: Option<String>,
}

impl QrLogin {
    /// A sign-in just sent as `request`.
    pub fn started(request: RequestId) -> Self {
        Self {
            request: Some(request),
            step: QrLoginStep::Starting,
            code: TextEdit::new(""),
            answered: false,
            error: None,
        }
    }

    /// Takes the engine's step for `request`; another command's step is not this one's (false).
    pub fn moved(&mut self, request: RequestId, step: QrLoginStep) -> bool {
        if self.request != Some(request) {
            return false;
        }
        if step == QrLoginStep::EnterCheckCode && self.step != step {
            self.code = super::focused(TextEdit::new(""));
        }
        self.step = step;
        self.answered = false;
        true
    }

    /// Records the failure of `request`; another command's failure is not this one's (false).
    pub fn fail(&mut self, request: RequestId, error: String) -> bool {
        if self.request != Some(request) {
            return false;
        }
        self.request = None;
        self.answered = false;
        self.error = Some(error);
        true
    }

    /// Whether the sign-in is over: done, or failed.
    pub fn ended(&self) -> bool {
        self.request.is_none() || matches!(self.step, QrLoginStep::Done { .. })
    }

    /// The check code typed in, once it is two digits.
    pub fn check_code(&self) -> Option<u8> {
        let typed = self.code.text.trim();
        if typed.len() != 2 || !typed.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        typed.parse().ok()
    }

    /// The check code to answer with, marking it answered: once per step, and only while asked.
    pub fn answer(&mut self) -> Option<(RequestId, u8)> {
        let request = self.request?;
        if self.step != QrLoginStep::EnterCheckCode || self.answered {
            return None;
        }
        let code = self.check_code()?;
        self.answered = true;
        Some((request, code))
    }
}

#[cfg(test)]
mod tests;
