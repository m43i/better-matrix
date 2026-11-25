//! What an [`Outgoing`] message becomes on the wire. The timeline, the room's send queue and
//! scheduled sends all build their content here, so a message reads the same whichever way it
//! goes.

use matrix_sdk::ruma::events::AnyMessageLikeEventContent;
use matrix_sdk::ruma::events::room::message::{
    LocationMessageEventContent, MessageType, RoomMessageEventContent, RoomMessageEventContentWithoutRelation,
};

use crate::Error;
use crate::model::{Mentions, Outgoing, PackImage};

/// `m.mentions` for what the app says a message mentions. Always sent, empty or not: a message
/// carrying it mentions only whom it names, never someone whose name its text happens to hold.
pub(crate) fn mentions(mentions: &Mentions) -> matrix_sdk::ruma::events::Mentions {
    let users = mentions.user_ids.iter().filter_map(|id| matrix_sdk::ruma::UserId::parse(id).ok());
    let mut out = matrix_sdk::ruma::events::Mentions::with_user_ids(users);
    out.room = mentions.room;
    out
}

/// A text message's content: formatted when it has HTML.
pub(crate) fn text(body: &str, html: Option<&str>, mentioned: &Mentions) -> RoomMessageEventContentWithoutRelation {
    let content = match html {
        Some(html) => RoomMessageEventContentWithoutRelation::text_html(body, html),
        None => RoomMessageEventContentWithoutRelation::text_plain(body),
    };
    content.add_mentions(mentions(mentioned))
}

/// The room message `message` is, for the kinds that are one (text, emotes, locations, GIFs):
/// these may answer another message or join a thread.
pub(crate) fn room_message(message: &Outgoing) -> Option<RoomMessageEventContentWithoutRelation> {
    Some(match message {
        Outgoing::Text { body, html, mentions } => text(body, html.as_deref(), mentions),
        Outgoing::Gif(gif) => text(&gif.url, None, &Mentions::default()),
        Outgoing::Emote { body, html, mentions: mentioned } => {
            let content = match html {
                Some(html) => RoomMessageEventContentWithoutRelation::emote_html(body, html),
                None => RoomMessageEventContentWithoutRelation::emote_plain(body),
            };
            content.add_mentions(mentions(mentioned))
        }
        Outgoing::Location { geo_uri, description } => {
            let body = match description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
                Some(description) => description.to_owned(),
                None => crate::geo::describe(geo_uri),
            };
            let content = LocationMessageEventContent::new(body, geo_uri.clone())
                .with_asset_type(matrix_sdk::ruma::events::location::AssetType::Pin);
            RoomMessageEventContentWithoutRelation::new(MessageType::Location(content))
                .add_mentions(matrix_sdk::ruma::events::Mentions::new())
        }
        Outgoing::File(_) | Outgoing::Voice { .. } | Outgoing::Poll { .. } | Outgoing::Sticker(_) => return None,
    })
}

/// The event `message` is, for every kind but uploads (files and voice memos), which need their
/// file uploaded first.
pub(crate) fn event(message: &Outgoing) -> Result<Option<AnyMessageLikeEventContent>, Error> {
    if let Some(content) = room_message(message) {
        let content: RoomMessageEventContent = content.into();
        return Ok(Some(content.into()));
    }
    Ok(match message {
        Outgoing::Poll { question, answers, disclosed } => Some(crate::polls::start(question, answers, *disclosed)?),
        Outgoing::Sticker(image) => Some(AnyMessageLikeEventContent::Sticker(sticker(image)?)),
        _ => None,
    })
}

/// A pack's image as a sticker event.
fn sticker(image: &PackImage) -> Result<matrix_sdk::ruma::events::sticker::StickerEventContent, Error> {
    use matrix_sdk::ruma::UInt;
    use matrix_sdk::ruma::events::room::ImageInfo;
    let url = matrix_sdk::ruma::OwnedMxcUri::from(image.url.as_str());
    if !url.is_valid() {
        return Err(Error::MessageUnavailable);
    }
    let mut info = ImageInfo::new();
    if let Some((width, height)) = image.dimensions {
        info.width = Some(UInt::from(width));
        info.height = Some(UInt::from(height));
    }
    info.mimetype = image.mimetype.clone();
    Ok(matrix_sdk::ruma::events::sticker::StickerEventContent::new(image.body.clone(), info, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(message: &Outgoing) -> serde_json::Value {
        let content = event(message).expect("content").expect("an event");
        serde_json::to_value(&content).expect("serialized")
    }

    #[test]
    fn a_text_should_carry_whom_it_mentions() {
        let message = Outgoing::Text {
            body: "Ada: hi".into(),
            html: Some("<a href=\"https://matrix.to/#/@ada:x.org\">Ada</a>: hi".into()),
            mentions: Mentions { user_ids: vec!["@ada:x.org".into()], room: false },
        };
        assert_eq!(json(&message)["m.mentions"], serde_json::json!({ "user_ids": ["@ada:x.org"] }));
    }

    #[test]
    fn a_text_mentioning_nobody_should_still_say_so() {
        let message = Outgoing::Text { body: "Ada said hi".into(), html: None, mentions: Mentions::default() };
        assert_eq!(json(&message)["m.mentions"], serde_json::json!({}));
    }

    #[test]
    fn a_room_mention_should_mention_the_room() {
        let message =
            Outgoing::Text { body: "@room hi".into(), html: None, mentions: Mentions { user_ids: vec![], room: true } };
        assert_eq!(json(&message)["m.mentions"]["room"], true);
    }

    #[test]
    fn an_emote_should_go_as_one() {
        let message = Outgoing::Emote { body: "waves".into(), html: None, mentions: Mentions::default() };
        assert_eq!(
            (json(&message)["msgtype"].as_str(), json(&message)["body"].as_str()),
            (Some("m.emote"), Some("waves"))
        );
    }

    #[test]
    fn a_location_should_carry_its_uri_and_name() {
        let message =
            Outgoing::Location { geo_uri: "geo:52.5163,13.3777".into(), description: Some("Brandenburger Tor".into()) };
        let content = json(&message);
        assert_eq!(content["msgtype"], "m.location");
        assert_eq!(content["geo_uri"], "geo:52.5163,13.3777");
        assert_eq!(content["body"], "Brandenburger Tor");
        assert_eq!(content["org.matrix.msc3488.asset"]["type"], "m.pin");
    }

    #[test]
    fn a_location_without_a_name_should_describe_its_coordinates() {
        let message = Outgoing::Location { geo_uri: "geo:52.5163,13.3777".into(), description: None };
        assert_eq!(json(&message)["body"], "Location: 52.5163, 13.3777");
    }

    #[test]
    fn a_gif_should_go_as_its_link() {
        let gif = crate::model::FoundGif {
            id: "a1".into(),
            title: "A cat waves".into(),
            url: "https://giphy.com/gifs/cat-a1".into(),
            preview: "https://media.giphy.com/media/a1/200w.gif".into(),
            preview_size: None,
            sent_ping: None,
        };
        let content = json(&Outgoing::Gif(gif));
        assert_eq!(
            (content["msgtype"].as_str(), content["body"].as_str()),
            (Some("m.text"), Some("https://giphy.com/gifs/cat-a1"))
        );
    }

    #[test]
    fn a_sticker_should_go_as_a_sticker_event() {
        let image = PackImage {
            shortcode: "wave".into(),
            body: "A waving cat".into(),
            url: "mxc://x.org/cat".into(),
            media: crate::MediaRef::opaque("cat"),
            emoticon: false,
            sticker: true,
            dimensions: Some((256, 256)),
            mimetype: Some("image/png".into()),
        };
        let content = json(&Outgoing::Sticker(image));
        assert_eq!(
            (content["url"].as_str(), content["body"].as_str()),
            (Some("mxc://x.org/cat"), Some("A waving cat"))
        );
        assert_eq!(content["info"]["w"], 256);
    }
}
