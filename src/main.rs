use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::TitleBar;
use gpui_platform::application;
use retranslate::{
    app_state::AppState,
    logging,
    storage::{self, Database},
    ui::MainWindow,
};
use std::sync::Arc;

fn main() {
    // 1. Initialize logging subsystem (logs/app.log, API key redaction)
    logging::init();
    tracing::info!("Starting reTranslate desktop application");

    // 2. Initialize isolated multi-threaded Tokio background runtime
    let tokio_runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("retranslate-tokio-worker")
            .enable_all()
            .build()
            .expect("Failed to initialize background Tokio runtime"),
    );

    let runtime_handle = tokio_runtime.handle().clone();

    // 3. Initialize SQLite database & Credential Store
    let db_path = Database::default_path().unwrap_or_else(|_| "retranslate.db".into());
    let _db = Database::open(&db_path).unwrap_or_else(|_| {
        Database::open_in_memory().expect("Fallback in-memory DB failed")
    });
    let _credentials = storage::CredentialStore::new().ok();

    // Spawn a background task verifying the async Tokio bridge
    runtime_handle.spawn(async {
        tracing::info!("Tokio background runtime bridge active");
    });

    // 4. Launch GPUI application on Main Thread
    application().run(move |cx: &mut App| {
        // Initialize GPUI styled component suite
        gpui_component::init(cx);

        // Initialize AppState model
        let handle_for_app = runtime_handle.clone();
        let app_state = cx.new(|cx| AppState::new(handle_for_app, cx));
        cx.set_global(retranslate::GlobalAppState(app_state));

        let window_size = size(px(1200.0), px(800.0));
        let bounds = Bounds::centered(None, window_size, cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitleBar::title_bar_options()),
            app_owns_titlebar_drag: true,
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            // Mandatory gpui_base::Root wrapping on primary window view
            let view = cx.new(|cx| MainWindow::new(window, cx));
            cx.new(|cx| gpui_base::Root::new(view, window, cx))
        })
        .expect("Failed to open primary application window");

        cx.activate(true);
    });
}
