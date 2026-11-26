//! Desktop device fixes and MapLibre Native snapshots for Rune's image cache.
use crate::{Error, MediaRef, MediaSize};
use maplibre_native_ffi as mln;
use std::{path::Path, time::Duration};
#[cfg(target_os = "macos")]
use waterkit_permission::{Permission, PermissionStatus};

#[cfg(not(target_os = "linux"))]
pub(crate) async fn position(request: bool) -> Result<String, Error> {
    #[cfg(target_os = "windows")]
    let _ = request; // The Rune window owns the foreground permission prompt.
    #[cfg(target_os = "macos")]
    if request {
        let access =
            waterkit_permission::request(Permission::Location).await.map_err(|e| Error::Location(e.to_string()))?;
        if !matches!(access, PermissionStatus::Granted | PermissionStatus::Unknown) {
            return Err(Error::Location("Location access is unavailable".into()));
        }
    }
    // Windows' permission check calls RequestAccessAsync, which can prompt. Its position
    // query instead returns E_ACCESSDENIED when the app has no existing grant.
    #[cfg(target_os = "macos")]
    if !request && waterkit_permission::check(Permission::Location).await != PermissionStatus::Granted {
        return Err(Error::Location("Location access is unavailable".into()));
    }
    let fix = tokio::time::timeout(Duration::from_secs(10), waterkit_location::Location::get())
        .await
        .map_err(|_| Error::Location("Location timed out".into()))?
        .map_err(|e| Error::Location(e.to_string()))?;
    Ok(super::uri(fix.latitude().get(), fix.longitude().get()))
}

#[cfg(target_os = "linux")]
pub(crate) async fn position(request: bool) -> Result<String, Error> {
    use ashpd::desktop::location::{Accuracy, CreateSessionOptions, LocationProxy};
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    // The portal checks permission at Start. Only an explicit first request may open its prompt.
    static GRANTED: AtomicBool = AtomicBool::new(false);
    if !request && !GRANTED.load(Ordering::Relaxed) {
        return Err(Error::Location("Device location has not been requested".into()));
    }
    let fix = async {
        // A private bus connection owns just this session. Cancellation drops it and ends the
        // portal session, including a request still waiting on the permission dialog.
        let bus = ashpd::zbus::Connection::session().await.map_err(ashpd::Error::from)?;
        let portal = LocationProxy::with_connection(bus).await?;
        let session = portal.create_session(CreateSessionOptions::default().set_accuracy(Accuracy::Street)).await?;
        let mut fixes = portal.receive_location_updated().await?;
        portal.start(&session, None, Default::default()).await?.response()?;
        let result = fixes.next().await.map(|fix| super::uri(fix.latitude(), fix.longitude()));
        session.close().await?;
        Ok::<_, ashpd::Error>(result)
    };
    let result = tokio::time::timeout(Duration::from_secs(if request { 60 } else { 10 }), fix)
        .await
        .map_err(|_| Error::Location("Location timed out".into()))?
        .map_err(|e| Error::Location(e.to_string()))?
        .ok_or(Error::MessageUnavailable)?;
    GRANTED.store(true, Ordering::Relaxed);
    Ok(result)
}

pub(crate) fn is_map(media: &MediaRef) -> bool {
    media.0.starts_with(super::MAP_PREFIX) || media.0.starts_with("osm:")
}

pub(crate) async fn draw(cache: &Path, media: &MediaRef, size: MediaSize) -> Result<Vec<u8>, Error> {
    let (_, fields) = media.0.split_once(':').ok_or(Error::MessageUnavailable)?;
    let mut parts = fields.split(',');
    let lat = parts.next().ok_or(Error::MessageUnavailable)?;
    let lon = parts.next().ok_or(Error::MessageUnavailable)?;
    let (lat, lon) = super::coordinates(&format!("geo:{lat},{lon}")).ok_or(Error::MessageUnavailable)?;
    let zoom = parts.next().and_then(|z| z.parse::<u8>().ok()).filter(|z| *z <= 20).ok_or(Error::MessageUnavailable)?;
    let style = if parts.next() == Some("dark") {
        include_str!("../../maps/dark.json")
    } else {
        include_str!("../../maps/light.json")
    };
    let (width, height) = match size {
        MediaSize::Original => (640, 400),
        MediaSize::Thumbnail { width, height } => (width.clamp(32, 2048), height.clamp(32, 2048)),
    };
    tokio::fs::create_dir_all(cache).await.map_err(Error::Storage)?;
    let cache = cache.join("maplibre.db");
    // Each offscreen session owns a graphics device. Bound concurrent snapshots while scrolling.
    static RENDERER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let permit = RENDERER.acquire().await.map_err(|_| Error::Cancelled)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let pixels = snapshot(&cache, mln::LogicalSize::new(width, height), mln::LatLng::new(lat, lon), zoom, style)
            .map_err(|e| Error::Location(e.to_string()))?;
        let image = image::RgbaImage::from_raw(width, height, pixels).ok_or(Error::MessageUnavailable)?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).map_err(|e| Error::Location(e.to_string()))?;
        Ok(bytes.into_inner())
    })
    .await
    .map_err(|e| Error::Location(e.to_string()))?
}

fn snapshot(cache: &Path, size: mln::LogicalSize, center: mln::LatLng, zoom: u8, style: &str) -> mln::Result<Vec<u8>> {
    let runtime = mln::RuntimeHandle::create(Some(&mln::RuntimeOptions {
        cache_path: &cache.to_string_lossy(),
        asset_path: "",
    }))?;
    let options = mln::MapOptions {
        map_mode: mln::MapMode::Static,
        ..mln::MapOptions::new(mln::LogicalExtent::new(size.width, size.height, 1.0))
    };
    let (map, ready) = runtime.create_map(&options)?;
    ready.wait()?;
    let (session, attached) = map.attach_offscreen(
        &mln::OffscreenDescriptor::new(size),
        &mln::RenderSessionAttachOptions::new(mln::RenderDriverKind::CoreWorker),
        None,
    )?;
    attached.wait()?;
    map.discarding().set_style_json(style)?;
    map.discarding().update_camera(&mln::CameraUpdate {
        camera: mln::CameraOptions { center: Some(center), zoom: Some(f64::from(zoom)), ..Default::default() },
        ..Default::default()
    })?;
    map.request_still_image()?
        .wait_timeout(Duration::from_secs(30))
        .map_err(|_| mln::Error::invalid_argument("Map timed out"))??;
    let image = session.read_texture()?.wait()?;
    let row = size.width as usize * 4;
    let stride = image.info.stride as usize;
    if image.info.width != size.width
        || image.info.height != size.height
        || stride < row
        || image.data.len() != stride * size.height as usize
    {
        return Err(mln::Error::invalid_argument("Unexpected map texture dimensions"));
    }
    if stride == row {
        return Ok(image.data);
    }
    Ok(image.data.chunks_exact(stride).flat_map(|line| line[..row].iter().copied()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a GPU and OpenFreeMap"]
    async fn maplibre_should_render_a_street_map_in_the_requested_style() {
        let cache = std::env::temp_dir().join("better-matrix-maplibre-test");
        for dark in [false, true] {
            let media = crate::geo::styled_map("geo:52.516275,13.377704", dark, 15).unwrap();
            let png = draw(&cache, &media, MediaSize::Thumbnail { width: 480, height: 280 }).await.unwrap();
            let pixels = image::load_from_memory(&png).unwrap().into_rgba8();
            assert_eq!(pixels.dimensions(), (480, 280));
            assert!(pixels.pixels().map(|p| p.0).collect::<std::collections::HashSet<_>>().len() > 50);
            if let Ok(dir) = std::env::var("MAP_PROOF_DIR") {
                std::fs::write(Path::new(&dir).join(if dark { "dark.png" } else { "light.png" }), png).unwrap();
            }
        }
    }
}
