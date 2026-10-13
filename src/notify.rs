//! Native macOS notifications for items pushed through the local ingest API.
//!
//! `UNUserNotificationCenter` only works inside an app bundle, so everything here
//! is a no-op when running unbundled (e.g. `cargo run`).

use block2::RcBlock;
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

pub struct DelegateIvars {
    on_click: Box<dyn Fn(String)>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "BetterClipboardNotificationDelegate"]
    #[ivars = DelegateIvars]
    pub struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        // Show banners even while the palette is the active app.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            (self.ivars().on_click)(identifier);
            completion_handler.call(());
        }
    }
);

impl NotificationDelegate {
    fn new(on_click: Box<dyn Fn(String)>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { on_click });
        unsafe { msg_send![super(this), init] }
    }
}

pub fn available() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Installs the click handler. `on_click` receives the identifier passed to [`post`].
/// The center holds its delegate weakly, so the caller must keep the returned object alive.
pub fn install(on_click: impl Fn(String) + 'static) -> Option<Retained<NotificationDelegate>> {
    if !available() {
        return None;
    }
    let delegate = NotificationDelegate::new(Box::new(on_click));
    UNUserNotificationCenter::currentNotificationCenter()
        .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    Some(delegate)
}

/// Asks macOS for permission to post alerts. Only prompts the first time.
pub fn request_authorization() {
    if !available() {
        return;
    }
    let handler = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &handler,
        );
}

pub fn post(identifier: &str, title: &str, body: &str) {
    if !available() {
        return;
    }
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(identifier),
        &content,
        None,
    );
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
}
