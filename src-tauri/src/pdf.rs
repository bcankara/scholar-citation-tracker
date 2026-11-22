//! Prints the report page to a PDF with WebView2's native `PrintToPdf`.
//!
//! A hidden window loads `report.html`, which asks for the report JSON (`report_payload`),
//! builds the document and tells Rust it is done (`report_ready`). Then `ICoreWebView2_7::PrintToPdf`
//! runs on the window's WebView2 with A4 page settings, and the window is closed.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use crate::report::Job;
use crate::scheduler::lock;

const LABEL: &str = "report";

/// A4 in inches and the print margins.
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

#[cfg(not(windows))]
fn start_print(_wv: tauri::webview::PlatformWebview, _p: String, tx: oneshot::Sender<PrintResult>) {
    let _ = tx.send(Err("PDF printing needs WebView2 (Windows)".into()));
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

    close(app);
    *lock(&job.payload) = None;
    *lock(&job.ready) = None;
    result
}
