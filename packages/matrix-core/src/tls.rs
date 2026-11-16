//! Which certificates HTTPS trusts. Desktops and iOS ask the system (reqwest's platform
//! verifier). Android's platform verifier needs a Kotlin component and JNI set-up the app doesn't
//! ship yet, so there every client trusts Mozilla's root store instead (ADR 0010).

use matrix_sdk::ClientBuilder;
use matrix_sdk::reqwest;

/// A Matrix client builder that trusts what this platform should.
pub(crate) fn sdk(builder: ClientBuilder) -> ClientBuilder {
    #[cfg(target_os = "android")]
    let builder = builder.disable_built_in_root_certificates().add_root_certificates(mozilla_roots());
    builder
}

/// An HTTP client builder that trusts what this platform should.
pub(crate) fn http(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    #[cfg(target_os = "android")]
    let builder = builder.tls_certs_only(mozilla_roots());
    builder
}

#[cfg(target_os = "android")]
fn mozilla_roots() -> Vec<reqwest::Certificate> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().filter_map(|der| reqwest::Certificate::from_der(der).ok()).collect()
}

/// Installs rustls' crypto provider for this process, once; later calls do nothing. Both
/// providers are built in (the SDK asks for one, LiveKit for the other), so rustls can't choose
/// for itself: without this, a call's connection panics. Phones use ring: aws-lc's entropy
/// source crashes on Android (its sized-free hook resolves to non-code there), and ring is built
/// in anyway (ADR 0010).
pub(crate) fn install_provider() {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
}
