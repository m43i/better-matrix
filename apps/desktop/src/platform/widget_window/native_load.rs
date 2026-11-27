//! Native navigation outcomes that Wry's page-load callback does not report.

#[cfg(target_os = "macos")]
pub(super) use macos::watch;
#[cfg(target_os = "windows")]
pub(super) use windows::watch;

#[cfg(target_os = "windows")]
#[expect(unsafe_code, reason = "WebView2 COM calls run on its owning UI thread")]
mod windows {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN,
        COREWEBVIEW2_WEB_ERROR_STATUS_VALID_AUTHENTICATION_CREDENTIALS_REQUIRED,
        COREWEBVIEW2_WEB_ERROR_STATUS_VALID_PROXY_AUTHENTICATION_REQUIRED,
    };
    use webview2_com::NavigationCompletedEventHandler;
    use wry::{WebView, WebViewExtWindows};

    pub(in super::super) fn watch(view: &WebView, completed: impl Fn(bool) + 'static) -> ::windows::core::Result<()> {
        let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut success = ::windows::core::BOOL::default();
            unsafe { args.IsSuccess(&mut success)? };
            if success.as_bool() {
                completed(true);
                return Ok(());
            }
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
            unsafe { args.WebErrorStatus(&mut status)? };
            // Cancelling a navigation is normal when it goes to the browser or is replaced.
            // Authentication challenges can be retried by the native web view.
            if !matches!(
                status,
                COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
                    | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_AUTHENTICATION_CREDENTIALS_REQUIRED
                    | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_PROXY_AUTHENTICATION_REQUIRED
            ) {
                completed(false);
            }
            Ok(())
        }));
        // WebView2 retains the handler until this helper's web view is destroyed.
        unsafe { view.webview().add_NavigationCompleted(&handler, &mut 0) }
    }
}

#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    reason = "WKWebView delegate installation and Objective-C message forwarding are main-thread only"
)]
mod macos {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, ProtocolObject, Sel};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_foundation::{NSError, NSObjectProtocol, NSURLErrorCancelled, NSURLErrorDomain};
    use objc2_web_kit::{WKNavigation, WKNavigationDelegate, WKWebView};
    use wry::{WebView, WebViewExtMacOS};

    struct Callbacks {
        wry: Retained<ProtocolObject<dyn WKNavigationDelegate>>,
        failed: Box<dyn Fn()>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "BetterMatrixNavigationDelegate"]
        #[thread_kind = MainThreadOnly]
        #[ivars = Callbacks]
        struct NavigationDelegate;

        unsafe impl NSObjectProtocol for NavigationDelegate {
            #[unsafe(method(respondsToSelector:))]
            fn responds_to(&self, selector: Sel) -> bool {
                self.ivars().wry.respondsToSelector(selector)
                    || unsafe { msg_send![super(self), respondsToSelector: selector] }
            }
        }

        impl NavigationDelegate {
            // Wry still owns all navigation policy, bridge lifecycle and download decisions.
            #[unsafe(method(forwardingTargetForSelector:))]
            fn forward(&self, _selector: Sel) -> &ProtocolObject<dyn WKNavigationDelegate> {
                &self.ivars().wry
            }
        }

        unsafe impl WKNavigationDelegate for NavigationDelegate {
            #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
            fn provisional_failed(&self, _view: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
                self.report(error);
            }

            #[unsafe(method(webView:didFailNavigation:withError:))]
            fn navigation_failed(&self, _view: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
                self.report(error);
            }
        }
    );

    impl NavigationDelegate {
        #[expect(
            deprecated,
            reason = "WebKit still reports its legacy policy interruption error for blocked navigation"
        )]
        fn report(&self, error: &NSError) {
            use objc2_web_kit::{WebKitErrorDomain, WebKitErrorFrameLoadInterruptedByPolicyChange};
            let domain = error.domain();
            let cancelled = error.code() == NSURLErrorCancelled && domain.isEqualToString(unsafe { NSURLErrorDomain });
            let interrupted = error.code() == WebKitErrorFrameLoadInterruptedByPolicyChange as isize
                && unsafe { WebKitErrorDomain }.is_some_and(|webkit| domain.isEqualToString(webkit));
            if !cancelled && !interrupted {
                (self.ivars().failed)();
            }
        }
    }

    pub(in super::super) fn watch(
        view: &WebView,
        failed: impl Fn() + 'static,
    ) -> Result<Retained<ProtocolObject<dyn WKNavigationDelegate>>, &'static str> {
        let main = MainThreadMarker::new().ok_or("widget web view isn't on the main thread")?;
        let webview = view.webview();
        let wry = unsafe { webview.navigationDelegate() }.ok_or("widget web view has no navigation delegate")?;
        let proxy = main.alloc::<NavigationDelegate>().set_ivars(Callbacks { wry, failed: Box::new(failed) });
        let proxy: Retained<NavigationDelegate> = unsafe { msg_send![super(proxy), init] };
        let proxy = ProtocolObject::from_retained(proxy);
        unsafe { webview.setNavigationDelegate(Some(&proxy)) };
        Ok(proxy)
    }
}
