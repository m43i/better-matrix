//! The older one-to-one standard (MSC2746, version 1): the two devices connect directly over
//! WebRTC, using the homeserver's TURN servers when they can't reach each other, and signal
//! through `m.call.*` events in their direct chat. The peer connection comes from LiveKit's
//! runtime, so it shares the microphone and speakers with MatrixRTC calls.

use livekit::rtc_engine::lk_runtime::LkRuntime;
use livekit::webrtc::ice_candidate::IceCandidate as RtcIceCandidate;
use livekit::webrtc::peer_connection::{AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState, TrackEvent};
use livekit::webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, IceServer, IceTransportsType, RtcConfiguration,
};
use livekit::webrtc::prelude::MediaStreamTrack;
use livekit::webrtc::session_description::{SdpType, SessionDescription};
use matrix_sdk::Room;
use matrix_sdk::ruma::api::client::voip::get_turn_server_info;
use tokio::sync::mpsc::UnboundedSender;

use super::CallError;
use super::signals::IceCandidate;

/// How long an invite rings before it lapses, in milliseconds.
pub(crate) const INVITE_LIFETIME: u64 = 60_000;
/// The standard's version the app speaks.
const VERSION: &str = "1";

/// What the peer connection reports, on the call's task.
pub(crate) enum PeerEvent {
    /// A local candidate to send the other side.
    Candidate(IceCandidate),
    /// A track from the other side: audio plays by itself, video goes to a sink.
    Track(MediaStreamTrack),
    /// The connection's state.
    State(PeerConnectionState),
}

/// The connection to the other device.
pub(crate) struct Direct {
    pub call_id: String,
    /// The user's side: this device.
    pub party_id: String,
    /// The other side's party, once it answered.
    pub peer: Option<String>,
    pc: PeerConnection,
    /// The connection got through at least once, as WebRTC said, ahead of its queued events.
    reached: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Their candidates that came before their description.
    early: Vec<IceCandidate>,
    described: bool,
}

impl Direct {
    /// A connection for call `call_id`, reporting to `events`.
    pub async fn new(
        room: &Room,
        call_id: String,
        party_id: String,
        events: UnboundedSender<PeerEvent>,
    ) -> Result<Self, CallError> {
        // A server slow to name its TURN servers doesn't hold the call up: it goes without.
        let request = get_turn_server_info::v3::Request::new();
        let turn = room.client().send(request).with_request_config(super::rtc::quick()).await.ok();
        let ice_servers = turn
            .map(|turn| vec![IceServer { urls: turn.uris, username: turn.username, password: turn.password }])
            .unwrap_or_default();
        let mut config = RtcConfiguration::default();
        config.ice_servers = ice_servers;
        // Gathered once, as browsers do: gathering continually (LiveKit's default, for its SFU)
        // keeps a connection that can't get through checking forever, never failing.
        config.continual_gathering_policy = ContinualGatheringPolicy::GatherOnce;
        config.ice_transport_type = IceTransportsType::All;
        let pc = LkRuntime::instance()
            .pc_factory()
            .create_peer_connection(config)
            .map_err(|error| CallError::Media(error.to_string()))?;
        let sender = events.clone();
        pc.on_ice_candidate(Some(Box::new(move |candidate: RtcIceCandidate| {
            let _ = sender.send(PeerEvent::Candidate(IceCandidate {
                candidate: candidate.candidate(),
                sdp_mid: candidate.sdp_mid(),
                sdp_m_line_index: candidate.sdp_mline_index(),
            }));
        })));
        let sender = events.clone();
        pc.on_track(Some(Box::new(move |event: TrackEvent| {
            let _ = sender.send(PeerEvent::Track(event.track));
        })));
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reaching = std::sync::Arc::clone(&reached);
        pc.on_connection_state_change(Some(Box::new(move |state| {
            if state == PeerConnectionState::Connected {
                reaching.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let _ = events.send(PeerEvent::State(state));
        })));
        Ok(Self { call_id, party_id, peer: None, pc, reached, early: Vec::new(), described: false })
    }

    /// Sends the user's media: the microphone, and the camera when there is one.
    pub fn add_tracks(&self, tracks: impl IntoIterator<Item = MediaStreamTrack>) -> Result<(), CallError> {
        for track in tracks {
            self.pc.add_track(track, &["stream"]).map_err(|error| CallError::Media(error.to_string()))?;
        }
        Ok(())
    }

    /// The offer for an invite: video only for a video call, so a voice call doesn't ring as one.
    pub async fn offer(&self, video: bool) -> Result<String, CallError> {
        let options = OfferOptions {
            offer_to_receive_audio: Some(true),
            offer_to_receive_video: Some(video),
            ..OfferOptions::default()
        };
        let offer = self.pc.create_offer(options).await.map_err(|error| CallError::Media(error.to_string()))?;
        let sdp = offer.to_string();
        self.pc.set_local_description(offer).await.map_err(|error| CallError::Media(error.to_string()))?;
        Ok(sdp)
    }

    /// Takes the caller's offer and returns the answer.
    pub async fn accept(&mut self, offer: &str) -> Result<String, CallError> {
        self.describe(offer, SdpType::Offer).await?;
        let answer = self
            .pc
            .create_answer(AnswerOptions::default())
            .await
            .map_err(|error| CallError::Media(error.to_string()))?;
        let sdp = answer.to_string();
        self.pc.set_local_description(answer).await.map_err(|error| CallError::Media(error.to_string()))?;
        Ok(sdp)
    }

    /// Takes the callee's answer.
    pub async fn answered(&mut self, answer: &str) -> Result<(), CallError> {
        self.describe(answer, SdpType::Answer).await
    }

    async fn describe(&mut self, sdp: &str, kind: SdpType) -> Result<(), CallError> {
        let description = SessionDescription::parse(sdp, kind).map_err(|error| CallError::Media(error.to_string()))?;
        self.pc.set_remote_description(description).await.map_err(|error| CallError::Media(error.to_string()))?;
        self.described = true;
        for candidate in std::mem::take(&mut self.early) {
            self.add(&candidate).await;
        }
        Ok(())
    }

    /// The other side's candidates; kept until their description arrived.
    pub async fn candidates(&mut self, candidates: Vec<IceCandidate>) {
        if !self.described {
            self.early.extend(candidates);
            return;
        }
        for candidate in &candidates {
            self.add(candidate).await;
        }
    }

    async fn add(&self, candidate: &IceCandidate) {
        if candidate.candidate.is_empty() {
            return;
        }
        if let Ok(parsed) = RtcIceCandidate::parse(&candidate.sdp_mid, candidate.sdp_m_line_index, &candidate.candidate)
        {
            let _ = self.pc.add_ice_candidate(parsed).await;
        }
    }

    /// Whether the connection got through at least once, whatever its events still on their
    /// way say (it may have dropped since, and be coming back).
    pub fn got_through(&self) -> bool {
        self.reached.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn close(&self) {
        self.pc.close();
    }
}

fn base(call_id: &str, party_id: &str) -> serde_json::Value {
    serde_json::json!({ "call_id": call_id, "party_id": party_id, "version": VERSION })
}

fn with(mut content: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    if let (Some(content), serde_json::Value::Object(extra)) = (content.as_object_mut(), extra) {
        content.extend(extra);
    }
    content
}

async fn send(room: &Room, kind: &str, content: serde_json::Value) -> Result<(), CallError> {
    room.send_raw(kind, content).await.map(|_| ()).map_err(|error| CallError::Service(error.to_string()))
}

pub(crate) async fn invite(room: &Room, call: &Direct, sdp: String, invitee: &str) -> Result<(), CallError> {
    let content = with(
        base(&call.call_id, &call.party_id),
        serde_json::json!({ "lifetime": INVITE_LIFETIME, "offer": { "type": "offer", "sdp": sdp }, "invitee": invitee }),
    );
    send(room, "m.call.invite", content).await
}

pub(crate) async fn answer(room: &Room, call: &Direct, sdp: String) -> Result<(), CallError> {
    let content =
        with(base(&call.call_id, &call.party_id), serde_json::json!({ "answer": { "type": "answer", "sdp": sdp } }));
    send(room, "m.call.answer", content).await
}

pub(crate) async fn select_answer(room: &Room, call: &Direct, selected: &str) -> Result<(), CallError> {
    let content = with(base(&call.call_id, &call.party_id), serde_json::json!({ "selected_party_id": selected }));
    send(room, "m.call.select_answer", content).await
}

pub(crate) async fn candidates(
    room: &Room,
    call_id: &str,
    party_id: &str,
    candidates: &[IceCandidate],
) -> Result<(), CallError> {
    let list: Vec<_> = candidates
        .iter()
        .map(|c| serde_json::json!({ "candidate": c.candidate, "sdpMid": c.sdp_mid, "sdpMLineIndex": c.sdp_m_line_index }))
        .collect();
    send(room, "m.call.candidates", with(base(call_id, party_id), serde_json::json!({ "candidates": list }))).await
}

/// Tells the other side the call is over. Bounded: a hang-up mustn't wait out a dead network
/// (the other side's connection fails on its own then).
pub(crate) async fn hangup(room: &Room, call_id: &str, party_id: &str) {
    let content = with(base(call_id, party_id), serde_json::json!({ "reason": "user_hangup" }));
    let _ = tokio::time::timeout(HANGUP_WAIT, send(room, "m.call.hangup", content)).await;
}

/// How long sending a hangup may take.
const HANGUP_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) async fn reject(room: &Room, call_id: &str, party_id: &str) {
    let _ = send(room, "m.call.reject", base(call_id, party_id)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::has_video;

    #[test]
    fn an_offer_with_video_turned_off_should_count_as_voice() {
        assert!(!has_video("v=0\r\nm=audio 9 RTP/SAVPF 111\r\nm=video 0 RTP/SAVPF 96\r\n"));
        assert!(!has_video("v=0\r\nm=audio 9 RTP/SAVPF 111\r\nm=video 9 RTP/SAVPF 96\r\na=inactive\r\n"));
    }

    #[test]
    fn an_offer_only_asking_to_receive_video_should_count_as_voice() {
        let sdp =
            "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=sendrecv\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=recvonly\r\n";
        assert!(!has_video(sdp));
    }

    #[test]
    fn an_offer_with_a_video_line_should_count_as_video() {
        assert!(has_video("v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n"));
        assert!(!has_video("v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n"));
    }

    #[test]
    fn signalling_content_should_carry_the_call_party_and_version() {
        let content = with(base("c", "p"), serde_json::json!({ "lifetime": 1 }));
        assert_eq!(
            (content["call_id"].as_str(), content["version"].as_str(), content["lifetime"].as_u64()),
            (Some("c"), Some("1"), Some(1))
        );
    }
}
