//! Prints the report page to a PDF with the platform's own webview printing.
//!
//! A hidden window loads `report.html`, which asks for the report JSON (`report_payload`),
//! builds the document and tells Rust it is done (`report_ready`). Then the window's webview
//! prints itself to the file with A4 page settings, and the window is closed:
//! `ICoreWebView2_7::PrintToPdf` on Windows, `WKWebView.printOperationWithPrintInfo:` driven by
//! an `NSPrintOperation` with the "save to file" job disposition on macOS.

use std::path::Path;
use std::sync::Arc;
#[cfg(windows)]
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use crate::report::Job;
use crate::scheduler::lock;

const LABEL: &str = "report";

/// A4 in inches and the print margins (inches, as WebView2 wants them).
const PAGE_W: f64 = 8.27;
const PAGE_H: f64 = 11.69;
const MARGIN_TOP: f64 = 0.7;
const MARGIN_BOTTOM: f64 = 0.7;
const MARGIN_SIDE: f64 = 0.6;

type PrintResult = Result<(), String>;

/// Runs on the UI thread: configures A4 settings and starts `PrintToPdf`; the result arrives
/// through `tx` when WebView2 has written the file.
#[cfg(windows)]
fn start_print(wv: tauri::webview::PlatformWebview, path: String, tx: oneshot::Sender<PrintResult>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Environment6, ICoreWebView2_7, COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows::core::{Interface, HSTRING};

    let tx = Arc::new(Mutex::new(Some(tx)));
    let send = {
        let tx = tx.clone();
        move |r: PrintResult| {
            if let Some(t) = lock(&tx).take() {
                let _ = t.send(r);
            }
        }
    };
    let started = (|| -> windows::core::Result<()> {
        // SAFETY: plain COM calls on the window's own WebView2 objects, on the thread that owns them.
        unsafe {
            let core = wv.controller().CoreWebView2()?;
            let core7: ICoreWebView2_7 = core.cast()?;
            let env6: ICoreWebView2Environment6 = wv.environment().cast()?;
            let settings = env6.CreatePrintSettings()?;
            settings.SetOrientation(COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT)?;
            settings.SetPageWidth(PAGE_W)?;
            settings.SetPageHeight(PAGE_H)?;
            settings.SetMarginTop(MARGIN_TOP)?;
            settings.SetMarginBottom(MARGIN_BOTTOM)?;
            settings.SetMarginLeft(MARGIN_SIDE)?;
            settings.SetMarginRight(MARGIN_SIDE)?;
            settings.SetShouldPrintBackgrounds(true)?;
            // No browser header/footer (date, title, URI): the page draws its own footer with CSS @page boxes.
            settings.SetShouldPrintHeaderAndFooter(false)?;
            let done = send.clone();
            let handler = PrintToPdfCompletedHandler::create(Box::new(move |error, ok| {
                match error {
                    Ok(()) if ok => done(Ok(())),
                    Ok(()) => done(Err("PrintToPdf reported failure".into())),
                    Err(e) => done(Err(e.to_string())),
                }
                Ok(())
            }));
            core7.PrintToPdf(&HSTRING::from(path), &settings, &handler)?;
        }
        Ok(())
    })();
    if let Err(e) = started {
        send(Err(e.to_string()));
    }
}

#[cfg(target_os = "macos")]
use mac::start_print;

/// Printing the report on macOS.
///
/// `WKWebView` hands out an `NSPrintOperation` that knows how to paginate the page, Core Printing
/// points that job at a PDF file instead of a printer, and the operation runs asynchronously.
///
/// Both of those details are load-bearing. A job left pointing at a printer fails on a Mac with
/// none configured (`PMSessionBeginCGDocumentNoDialog` returns -30872) and AppKit answers with a
/// modal alert, which on a hidden window hangs the app for good. And `runOperation` blocks the
/// main thread, so the page rects WebKit computes in the web process never arrive over the run
/// loop; pagination then runs against the placeholder range `1 ..= NSIntegerMax` and writes pages
/// until the disk fills. Running it modal for the report window returns at once and reports
/// through `printOperationDidRun:success:contextInfo:`.
#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, define_class, msg_send, sel, AnyThread};
    use objc2_foundation::{NSObject, NSSize, NSString};
    use tokio::sync::oneshot;

    use super::{PrintResult, MARGIN_BOTTOM, MARGIN_SIDE, MARGIN_TOP, PAGE_H, PAGE_W};

    /// PostScript points per inch: AppKit measures pages in points, WebView2 in inches.
    const PT: f64 = 72.0;
    /// `NSPaperOrientationPortrait`.
    const PORTRAIT: isize = 0;
    /// `kPMDestinationFile`.
    const DESTINATION_FILE: u16 = 2;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        /// Core Printing: where the job goes. The session and settings belong to the `NSPrintInfo`.
        fn PMSessionSetDestination(
            session: *mut c_void,
            settings: *mut c_void,
            dest_type: u16,
            dest_format: *const c_void,
            dest_location: *const c_void,
        ) -> i32;
    }

    /// The print that is running now, with its generation. One at a time: `print_report` destroys
    /// the report window before it builds the next one, and clears this when it stops waiting.
    static PENDING: Mutex<Option<(usize, oneshot::Sender<PrintResult>)>> = Mutex::new(None);
    /// Generation of the latest print. It travels to AppKit as the operation's `contextInfo` and
    /// comes back in the delegate call, so a callback that arrives after its export gave up (the
    /// 90 s timeout) cannot complete the export that started after it.
    static GENERATION: AtomicUsize = AtomicUsize::new(0);

    fn pending() -> std::sync::MutexGuard<'static, Option<(usize, oneshot::Sender<PrintResult>)>> {
        PENDING.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Answers the print of generation `generation`, if that is still the one waiting.
    fn finish(generation: usize, result: PrintResult) {
        let mut slot = pending();
        if slot.as_ref().is_some_and(|(g, _)| *g == generation) {
            if let Some((_, tx)) = slot.take() {
                let _ = tx.send(result);
            }
        }
    }

    /// Drops whatever print is still waiting; its caller has stopped listening (timeout or close).
    pub(super) fn cancel() {
        pending().take();
    }

    define_class!(
        // SAFETY: the superclass NSObject has no subclassing requirements, and this class holds
        // no instance variables and does not implement Drop.
        #[unsafe(super(NSObject))]
        #[name = "ScholarCitationTrackerPrintDelegate"]
        struct PrintDelegate;

        impl PrintDelegate {
            #[unsafe(method(printOperationDidRun:success:contextInfo:))]
            fn did_run(&self, _op: *mut AnyObject, success: Bool, ctx: *mut c_void) {
                finish(ctx.addr(), if success.as_bool() {
                    Ok(())
                } else {
                    Err("the print operation reported failure".to_string())
                });
            }
        }
    );

    /// One delegate for the life of the process: AppKit calls back into it after every export.
    fn delegate() -> *mut AnyObject {
        static PTR: OnceLock<usize> = OnceLock::new();
        *PTR.get_or_init(|| {
            let d: Retained<PrintDelegate> = unsafe { msg_send![PrintDelegate::alloc(), init] };
            Retained::into_raw(d) as usize
        }) as *mut AnyObject
    }

    /// Runs on the UI thread: starts the print and leaves the answer to the delegate.
    pub(super) fn start_print(
        wv: tauri::webview::PlatformWebview,
        path: String,
        tx: oneshot::Sender<PrintResult>,
    ) {
        let inner = wv.inner() as *mut AnyObject;
        let window = wv.ns_window() as *mut AnyObject;
        if inner.is_null() || window.is_null() {
            let _ = tx.send(Err("the report window has no webview".to_string()));
            return;
        }
        // A print that ended without reporting must not leave its caller waiting for this one.
        if let Some((_, old)) = pending().take() {
            let _ = old.send(Err("superseded by a new report".to_string()));
        }
        // Never 0, so a callback with a null contextInfo matches nothing.
        let generation = GENERATION.fetch_add(1, Ordering::Relaxed).wrapping_add(1).max(1);
        *pending() = Some((generation, tx));

        // SAFETY: plain Objective-C messages to the report window's own WKWebView and to AppKit
        // printing objects, on the main thread, which is the thread `with_webview` calls this on.
        unsafe {
            let webview: &AnyObject = &*inner;

            // A copy of the shared print info, so the user's own print settings stay untouched.
            // `ui/report.css` carries `@page { size: A4; margin: 0.7in 0.6in }`, which WebKit
            // honours when it lays the pages out; this puts the same paper on the job itself.
            let shared: Retained<AnyObject> = msg_send![class!(NSPrintInfo), sharedPrintInfo];
            let info: Retained<AnyObject> = msg_send![&*shared, copy];
            let _: () = msg_send![&*info, setOrientation: PORTRAIT];
            let _: () = msg_send![&*info, setPaperSize: NSSize::new(PAGE_W * PT, PAGE_H * PT)];
            let _: () = msg_send![&*info, setTopMargin: MARGIN_TOP * PT];
            let _: () = msg_send![&*info, setBottomMargin: MARGIN_BOTTOM * PT];
            let _: () = msg_send![&*info, setLeftMargin: MARGIN_SIDE * PT];
            let _: () = msg_send![&*info, setRightMargin: MARGIN_SIDE * PT];
            let _: () = msg_send![&*info, setHorizontallyCentered: false];
            let _: () = msg_send![&*info, setVerticallyCentered: false];
            let _: () = msg_send![&*info, setScalingFactor: 1.0f64];

            let session: *mut c_void = msg_send![&*info, PMPrintSession];
            let settings: *mut c_void = msg_send![&*info, PMPrintSettings];
            let pdf = NSString::from_str("application/pdf");
            let url: Retained<AnyObject> =
                msg_send![class!(NSURL), fileURLWithPath: &*NSString::from_str(&path)];
            let status = PMSessionSetDestination(
                session,
                settings,
                DESTINATION_FILE,
                Retained::as_ptr(&pdf) as *const c_void,
                Retained::as_ptr(&url) as *const c_void,
            );
            if status != 0 {
                finish(generation, Err(format!("the PDF destination was refused (PM status {status})")));
                return;
            }

            let op: Retained<AnyObject> = msg_send![webview, printOperationWithPrintInfo: &*info];
            let _: () = msg_send![&*op, setShowsPrintPanel: false];
            let _: () = msg_send![&*op, setShowsProgressPanel: false];
            let _: () = msg_send![&*op,
                runOperationModalForWindow: window,
                delegate: delegate(),
                didRunSelector: sel!(printOperationDidRun:success:contextInfo:),
                contextInfo: std::ptr::without_provenance_mut::<c_void>(generation)
            ];
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn start_print(_wv: tauri::webview::PlatformWebview, _p: String, tx: oneshot::Sender<PrintResult>) {
    let _ = tx.send(Err("PDF printing is implemented on Windows and macOS only".into()));
}

fn close(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

/// Builds the report in a hidden window and prints it to `out`.
pub async fn print_report(app: &AppHandle, job: &Arc<Job>, payload: String, out: &Path) -> PrintResult {
    close(app);
    let (ready_tx, ready_rx) = oneshot::channel::<()>();
    *lock(&job.payload) = Some(payload);
    *lock(&job.ready) = Some(ready_tx);

    let result = async {
        let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("report.html".into()))
            .title("Report")
            .inner_size(900.0, 1200.0)
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            .build()
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_secs(25), ready_rx)
            .await
            .map_err(|_| "the report page did not finish rendering".to_string())?
            .map_err(|_| "the report page was closed".to_string())?;
        // Let layout and fonts settle before printing.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let (tx, rx) = oneshot::channel::<PrintResult>();
        let path = out.to_string_lossy().to_string();
        win.with_webview(move |wv| start_print(wv, path, tx))
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_secs(90), rx)
            .await
            .map_err(|_| "printing timed out".to_string())?
            .map_err(|_| "printing was cancelled".to_string())?
    }
    .await;

    // A print that timed out may still call back later; it must not answer the next export.
    #[cfg(target_os = "macos")]
    mac::cancel();
    close(app);
    *lock(&job.payload) = None;
    *lock(&job.ready) = None;
    result
}
