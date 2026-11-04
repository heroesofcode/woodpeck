//! Browser entry point: restores the database from `IndexedDB`, starts the egui web runner, and
//! saves the database back whenever it changes. Only the shell lives here; training, review,
//! storage and sync logic stay in the shared modules so another web UI can reuse them.
use crate::{service::AppService, store::Store};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};

const DATABASE: &str = "woodpeck";
const OBJECTS: &str = "files";
const KEY: &str = "woodpeck.db";
/// How often the page checks for changes to save; saving is skipped when nothing changed.
const SAVE_EVERY_MS: u32 = 2_000;

fn js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
fn window() -> Result<web_sys::Window, JsValue> {
    web_sys::window().ok_or_else(|| js("No browser window."))
}
/// Resolves with the request's result once `IndexedDB` reports success.
async fn finished(request: &IdbRequest) -> Result<JsValue, JsValue> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let target = request.clone();
        let success = Closure::once_into_js(move || {
            let _ = resolve.call1(
                &JsValue::NULL,
                &target.result().unwrap_or(JsValue::UNDEFINED),
            );
        });
        let failure = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &js("IndexedDB request failed."));
        });
        request.set_onsuccess(Some(success.unchecked_ref()));
        request.set_onerror(Some(failure.unchecked_ref()));
    });
    JsFuture::from(promise).await
}
async fn open() -> Result<IdbDatabase, JsValue> {
    let factory = window()?
        .indexed_db()?
        .ok_or_else(|| js("IndexedDB is not available in this browser."))?;
    let request = factory.open_with_u32(DATABASE, 1)?;
    let upgrade = request.clone();
    let create = Closure::once_into_js(move || {
        if let Ok(result) = upgrade.result()
            && let Ok(database) = result.dyn_into::<IdbDatabase>()
            && !database.object_store_names().contains(OBJECTS)
        {
            let _ = database.create_object_store(OBJECTS);
        }
    });
    request.set_onupgradeneeded(Some(create.unchecked_ref()));
    finished(&request).await?.dyn_into()
}
async fn load(database: &IdbDatabase) -> Result<Option<Vec<u8>>, JsValue> {
    let objects = database
        .transaction_with_str(OBJECTS)?
        .object_store(OBJECTS)?;
    let value = finished(&objects.get(&JsValue::from_str(KEY))?).await?;
    Ok((!value.is_undefined() && !value.is_null())
        .then(|| js_sys::Uint8Array::new(&value).to_vec()))
}
/// Starts a write and calls `done` with whether the transaction committed.
fn save(
    database: &IdbDatabase,
    image: &[u8],
    done: impl FnOnce(bool) + 'static,
) -> Result<(), JsValue> {
    let transaction =
        database.transaction_with_str_and_mode(OBJECTS, IdbTransactionMode::Readwrite)?;
    transaction
        .object_store(OBJECTS)?
        .put_with_key(&js_sys::Uint8Array::from(image), &JsValue::from_str(KEY))?;
    let done = std::rc::Rc::new(std::cell::RefCell::new(Some(done)));
    let finish = move |ok: bool| {
        let done = done.clone();
        Closure::once_into_js(move || {
            if let Some(done) = done.borrow_mut().take() {
                done(ok);
            }
        })
    };
    let complete = finish(true);
    let failed = finish(false);
    transaction.set_oncomplete(Some(complete.unchecked_ref()));
    transaction.set_onerror(Some(failed.unchecked_ref()));
    transaction.set_onabort(Some(failed.unchecked_ref()));
    Ok(())
}
/// Asks the browser not to evict the database under storage pressure (Safari especially).
fn request_persistence() {
    if let Ok(window) = window()
        && let Ok(storage) = window.navigator().storage().persist()
    {
        drop(JsFuture::from(storage));
    }
}
fn canvas() -> Result<web_sys::HtmlCanvasElement, JsValue> {
    window()?
        .document()
        .and_then(|document| document.get_element_by_id("woodpeck"))
        .ok_or_else(|| js("Missing the woodpeck canvas."))?
        .dyn_into()
        .map_err(|_| js("The woodpeck element is not a canvas."))
}
fn loading(message: Option<&str>) {
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("loading"))
    {
        match message {
            Some(message) => element.set_text_content(Some(message)),
            None => element.remove(),
        }
    }
}
/// Saves after changes and when the page is hidden or closed, so a phone that kills the tab
/// keeps the last puzzle.
fn keep_saved(database: &IdbDatabase, service: crate::service::LocalService) {
    use std::{cell::Cell, rc::Rc};
    // A revision counts as saved only once IndexedDB commits it; one write runs at a time.
    let saved = Rc::new(Cell::new(None::<u64>));
    let writing = Rc::new(Cell::new(false));
    let flush = {
        let database = database.clone();
        move || {
            if writing.get() {
                return;
            }
            if let Ok(Some((revision, image))) = service.browser_image(saved.get()) {
                let (saved, writing_done) = (saved.clone(), writing.clone());
                writing.set(true);
                let started = save(&database, &image, move |ok| {
                    if ok {
                        saved.set(Some(revision));
                    }
                    writing_done.set(false);
                });
                if started.is_err() {
                    writing.set(false);
                }
            }
        }
    };
    let on_hide = {
        let flush = flush.clone();
        Closure::<dyn FnMut()>::new(flush)
    };
    if let Ok(window) = window() {
        let _ =
            window.add_event_listener_with_callback("pagehide", on_hide.as_ref().unchecked_ref());
        if let Some(document) = window.document() {
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                on_hide.as_ref().unchecked_ref(),
            );
        }
    }
    on_hide.forget();
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(SAVE_EVERY_MS).await;
            flush();
        }
    });
}
async fn run() -> Result<(), JsValue> {
    request_persistence();
    let database = open().await?;
    let image = load(&database).await?;
    let store = Store::from_image(image.as_deref()).map_err(js)?;
    let service = AppService::new(store).map_err(js)?;
    keep_saved(&database, (*service).clone());
    crate::app::run_web(canvas()?, service).await?;
    loading(None);
    Ok(())
}
/// Called from `main` on wasm32.
pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = run().await {
            let message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
            web_sys::console::error_1(&error);
            loading(Some(&format!("woodpeck could not start: {message}")));
        }
    });
}
/// Offers text as a file download.
pub fn download(name: &str, text: &str) -> Result<(), String> {
    let fail = |e: JsValue| format!("Download failed: {e:?}");
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/x-chess-pgn");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(fail)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(fail)?;
    let anchor: web_sys::HtmlAnchorElement = window()
        .map_err(fail)?
        .document()
        .ok_or("No document.")?
        .create_element("a")
        .map_err(fail)?
        .dyn_into()
        .map_err(|_| "Could not create a link.")?;
    anchor.set_href(&url);
    anchor.set_download(name);
    let body = window()
        .map_err(fail)?
        .document()
        .and_then(|d| d.body())
        .ok_or("No page body.")?;
    body.append_child(&anchor).map_err(fail)?;
    anchor.click();
    anchor.remove();
    // Revoke later so the browser has time to start reading the file.
    gloo_timers::callback::Timeout::new(60_000, move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    })
    .forget();
    Ok(())
}
