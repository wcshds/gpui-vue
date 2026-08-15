//! Desktop application bootstrap for native `gpui-vue` components.

use std::sync::Arc;

pub use gpui::{
    AnyWindowHandle, QuitMode, Result as DesktopResult, WindowBackgroundAppearance, WindowHandle,
    WindowKind,
};
use gpui::{
    App, Application, Entity, Pixels, Point, Render, SharedString, Size, TitlebarOptions, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::{
    assets::EmbeddedAssets,
    component::{NativeComponent, NativeComponentRoot},
    http::HttpClient,
};

/// A reusable application extension installed before the first window opens.
///
/// Plugins may register globals, actions, key bindings, menus, or other native
/// application services. Ordinary `FnOnce(&mut App)` callbacks implement this
/// trait automatically.
pub trait AppPlugin: 'static {
    /// Installs this plugin into the launched native application.
    fn install(self, app: &mut App);
}

impl<Install> AppPlugin for Install
where
    Install: FnOnce(&mut App) + 'static,
{
    fn install(self, app: &mut App) {
        self(app);
    }
}

/// Configuration for the first native desktop window.
#[allow(
    clippy::struct_excessive_bools,
    reason = "these independent native window capabilities intentionally remain explicit builder flags"
)]
#[derive(Clone, Debug, PartialEq)]
pub struct WindowConfig {
    /// Initial title shown by the platform title bar.
    title: SharedString,
    /// Initial logical content size.
    size: Size<Pixels>,
    /// Optional lower bound for interactive resizing.
    min_size: Option<Size<Pixels>>,
    /// Whether application content extends through the title bar.
    transparent_titlebar: bool,
    /// Optional macOS traffic-light position.
    traffic_light_position: Option<Point<Pixels>>,
    /// Whether the window receives focus when it opens.
    focus: bool,
    /// Whether the window is shown immediately after creation.
    show: bool,
    /// Native window kind.
    kind: WindowKind,
    /// Whether the platform permits moving the window.
    movable: bool,
    /// Whether the platform permits resizing the window.
    resizable: bool,
    /// Whether the platform permits minimizing the window.
    minimizable: bool,
    /// Platform background treatment.
    background: WindowBackgroundAppearance,
    /// Optional desktop-environment application identifier.
    app_id: Option<String>,
    /// Optional native tab group identifier.
    tabbing_identifier: Option<String>,
}

impl WindowConfig {
    /// Creates a centered window configuration in logical pixels.
    #[must_use]
    pub fn new(title: impl Into<SharedString>, width: f32, height: f32) -> Self {
        Self {
            title: title.into(),
            size: size(px(width), px(height)),
            min_size: None,
            transparent_titlebar: false,
            traffic_light_position: None,
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            movable: true,
            resizable: true,
            minimizable: true,
            background: WindowBackgroundAppearance::Opaque,
            app_id: None,
            tabbing_identifier: None,
        }
    }

    /// Sets the minimum resizable content size in logical pixels.
    #[must_use]
    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Some(size(px(width), px(height)));
        self
    }

    /// Selects whether content extends through the native title bar.
    #[must_use]
    pub const fn transparent_titlebar(mut self, transparent: bool) -> Self {
        self.transparent_titlebar = transparent;
        self
    }

    /// Positions the macOS traffic-light controls in logical pixels.
    #[must_use]
    pub fn traffic_light_position(mut self, x: f32, y: f32) -> Self {
        self.traffic_light_position = Some(point(px(x), px(y)));
        self
    }

    /// Selects whether the initial window receives focus when it opens.
    #[must_use]
    pub const fn focused(mut self, focus: bool) -> Self {
        self.focus = focus;
        self
    }

    /// Selects whether the initial window is visible immediately.
    #[must_use]
    pub const fn visible(mut self, show: bool) -> Self {
        self.show = show;
        self
    }

    /// Selects the native window kind, such as normal, popup, or floating.
    #[must_use]
    pub const fn kind(mut self, kind: WindowKind) -> Self {
        self.kind = kind;
        self
    }

    /// Selects whether the user can move the window.
    #[must_use]
    pub const fn movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
    }

    /// Selects whether the user can resize the window.
    #[must_use]
    pub const fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Selects whether the user can minimize the window.
    #[must_use]
    pub const fn minimizable(mut self, minimizable: bool) -> Self {
        self.minimizable = minimizable;
        self
    }

    /// Sets the platform background treatment for the window.
    #[must_use]
    pub const fn background(mut self, background: WindowBackgroundAppearance) -> Self {
        self.background = background;
        self
    }

    /// Sets the identifier used by desktop environments to group windows.
    #[must_use]
    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    /// Sets the macOS native tab group identifier.
    #[must_use]
    pub fn tabbing_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.tabbing_identifier = Some(identifier.into());
        self
    }

    /// Converts the stable `gpui-vue` configuration into native options.
    fn into_native_options(self, cx: &App) -> WindowOptions {
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(self.title),
                appears_transparent: self.transparent_titlebar,
                traffic_light_position: self.traffic_light_position,
            }),
            focus: self.focus,
            show: self.show,
            kind: self.kind,
            is_movable: self.movable,
            is_resizable: self.resizable,
            is_minimizable: self.minimizable,
            window_background: self.background,
            app_id: self.app_id,
            window_bounds: Some(WindowBounds::centered(self.size, cx)),
            window_min_size: self.min_size,
            tabbing_identifier: self.tabbing_identifier,
            ..WindowOptions::default()
        }
    }
}

/// Opens an additional native window whose root is a raw GPUI view.
///
/// This is the multi-window counterpart to [`DesktopApp::run`]. The supplied
/// [`WindowConfig`] is converted through the same stable `gpui-vue` window
/// policy as the initial window, while native creation errors are returned to
/// the caller. Clone the configuration before calling when the same policy is
/// reused for more than one window.
///
/// A raw view does not receive the visual lifecycle attachment generated by
/// [`crate::component!`]. Use [`open_component_window`] for generated native
/// components.
///
/// # Errors
///
/// Returns an error when the native platform cannot create the configured
/// window.
pub fn open_window<View, BuildRoot>(
    app: &mut App,
    config: WindowConfig,
    build_root: BuildRoot,
) -> DesktopResult<WindowHandle<View>>
where
    View: 'static + Render,
    BuildRoot: FnOnce(&mut Window, &mut App) -> Entity<View>,
{
    app.open_window(config.into_native_options(app), build_root)
}

/// Opens an additional native window whose root is a generated component.
///
/// The component is mounted through the same retained root host as
/// [`DesktopApp::run_component`], so its `mounted`, `updated`, and `unmounted`
/// hooks keep their normal visual-lifecycle semantics. The returned handle is
/// type-erased because the actual GPUI root is an internal lifecycle host, not
/// the component entity itself. It can still be used with GPUI's ordinary
/// window activation, update, and close APIs.
///
/// Native window creation errors are returned without panicking.
///
/// # Errors
///
/// Returns an error when the native platform cannot create the configured
/// window.
pub fn open_component_window<ComponentType, BuildRoot>(
    app: &mut App,
    config: WindowConfig,
    build_root: BuildRoot,
) -> DesktopResult<AnyWindowHandle>
where
    ComponentType: NativeComponent,
    BuildRoot: FnOnce(&mut Window, &mut App) -> Entity<ComponentType>,
{
    app.open_window(config.into_native_options(app), move |window, app| {
        let component = build_root(window, app);
        NativeComponentRoot::mount(component, app)
    })
    .map(Into::into)
}

/// Configured native application ready to mount a root component.
pub struct DesktopApp {
    /// Underlying platform application, hidden behind this bootstrap API.
    application: Application,
    /// Initial window configuration.
    window: WindowConfig,
    /// Application initialization callbacks, in registration order.
    setup: Vec<SetupCallback>,
}

/// One ordered application initializer retained until the platform launches.
type SetupCallback = Box<dyn FnOnce(&mut App)>;

impl DesktopApp {
    /// Creates a desktop application with one initial window.
    #[must_use]
    pub fn new(window: WindowConfig) -> Self {
        Self {
            application: Application::new(),
            window,
            setup: Vec::new(),
        }
    }

    /// Installs the HTTP transport used by native image elements.
    #[must_use]
    pub fn http_client(mut self, client: impl HttpClient) -> Self {
        self.application = self.application.with_http_client(Arc::new(client));
        self
    }

    /// Installs statically embedded application assets.
    ///
    /// Registered paths can be used by native image and SVG elements without
    /// requiring application code to import GPUI's asset-source trait.
    #[must_use]
    pub fn assets(mut self, assets: EmbeddedAssets) -> Self {
        self.application = self.application.with_assets(assets);
        self
    }

    /// Configures when the application exits automatically.
    #[must_use]
    pub fn quit_mode(mut self, mode: QuitMode) -> Self {
        self.application = self.application.with_quit_mode(mode);
        self
    }

    /// Registers a platform callback for URLs opened with this application.
    ///
    /// The platform owns the callback after registration and may invoke it
    /// more than once, including while the application is already running.
    /// URL delivery and registration policy remain platform-defined.
    #[must_use]
    pub fn on_open_urls<Handler>(self, handler: Handler) -> Self
    where
        Handler: FnMut(Vec<String>) + 'static,
    {
        self.application.on_open_urls(handler);
        self
    }

    /// Registers a callback for a platform request to reopen the application.
    ///
    /// On macOS this includes launching an already-running application from
    /// its Dock icon. The callback receives the live [`App`] and can use
    /// [`open_window`] or [`open_component_window`] when no suitable window is
    /// currently open. Other platforms decide when, or whether, to emit this
    /// lifecycle event.
    #[must_use]
    pub fn on_reopen<Handler>(self, handler: Handler) -> Self
    where
        Handler: FnMut(&mut App) + 'static,
    {
        self.application.on_reopen(handler);
        self
    }

    /// Requests runtime registration of one URL scheme after application launch.
    ///
    /// Supply the bare scheme name, such as `"gpui-vue"`, rather than a full
    /// URL. GPUI exposes registration on the live [`App`] instead of
    /// [`Application`], so this builder retains the owned scheme until setup,
    /// starts the returned native task, and logs any asynchronous platform
    /// error. Runtime registration is currently platform-dependent and may be
    /// unsupported or require a sufficiently recent operating system.
    #[must_use]
    pub fn register_url_scheme<Scheme>(self, scheme: Scheme) -> Self
    where
        Scheme: Into<String>,
    {
        let scheme = scheme.into();
        self.setup(move |app| {
            app.register_url_scheme(&scheme).detach_and_log_err(app);
        })
    }

    /// Registers an application setup callback.
    ///
    /// Setup callbacks run once, in registration order, after the platform has
    /// launched and before the initial window is opened. They are the native
    /// extension point for globals, actions, key bindings, menus, and reusable
    /// application plugins.
    #[must_use]
    pub fn setup(mut self, setup: impl FnOnce(&mut App) + 'static) -> Self {
        self.setup.push(Box::new(setup));
        self
    }

    /// Installs one reusable native application plugin.
    #[must_use]
    pub fn plugin<Plugin: AppPlugin>(self, plugin: Plugin) -> Self {
        self.setup(move |app| plugin.install(app))
    }

    /// Launches the platform event loop and mounts a raw GPUI root view.
    ///
    /// This compatibility entry point does not attach the visual lifecycle of
    /// a component generated by [`crate::component!`]. Use
    /// [`DesktopApp::run_component`] for a generated component root so its
    /// `mounted`, `updated`, and `unmounted` hooks follow the same semantics as
    /// nested component hosts.
    ///
    /// # Panics
    ///
    /// Panics when the platform cannot create the configured initial window.
    pub fn run<View>(self, build_root: impl 'static + FnOnce(&mut Window, &mut App) -> Entity<View>)
    where
        View: Render,
    {
        let window = self.window;
        let setup = self.setup;
        self.application.run(move |cx| {
            for initialize in setup {
                initialize(cx);
            }
            open_window(cx, window, build_root)
                .expect("gpui-vue failed to open the desktop window");
            cx.activate(true);
        });
    }

    /// Launches the platform event loop and mounts a generated component root.
    ///
    /// The root receives a retained visual host even though it has no parent
    /// element slot. Consequently its lifecycle hooks run with the same
    /// first-render, update-coalescing, and visual-teardown guarantees as a
    /// component embedded through [`crate::view!`]. Construction and `setup`
    /// still happen exactly once inside the supplied builder.
    ///
    /// # Panics
    ///
    /// Panics when the platform cannot create the configured initial window.
    pub fn run_component<ComponentType>(
        self,
        build_root: impl 'static + FnOnce(&mut Window, &mut App) -> Entity<ComponentType>,
    ) where
        ComponentType: NativeComponent,
    {
        let window = self.window;
        let setup = self.setup;
        self.application.run(move |cx| {
            for initialize in setup {
                initialize(cx);
            }
            open_component_window(cx, window, build_root)
                .expect("gpui-vue failed to open the desktop window");
            cx.activate(true);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext as _;

    struct TestRoot;

    impl Render for TestRoot {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }

    struct TestComponent;

    impl Render for TestComponent {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }

    impl NativeComponent for TestComponent {
        type Props = ();
        type Input = ();
        type MountState = ();

        fn construct(_input: Self::Input, _cx: &mut gpui::Context<Self>) -> Self {
            Self
        }

        fn reconcile_input(&mut self, _input: Self::Input, _cx: &mut gpui::Context<Self>) -> bool {
            false
        }
    }

    fn build_test_root(_window: &mut Window, app: &mut App) -> Entity<TestRoot> {
        app.new(|_| TestRoot)
    }

    fn build_test_component(_window: &mut Window, app: &mut App) -> Entity<TestComponent> {
        app.new(|_| TestComponent)
    }

    #[test]
    fn window_configuration_retains_desktop_policy() {
        let config = WindowConfig::new("Studio", 1280.0, 820.0)
            .min_size(980.0, 680.0)
            .transparent_titlebar(true)
            .traffic_light_position(14.0, 17.0);

        assert_eq!(config.title, SharedString::from("Studio"));
        assert_eq!(config.size, size(px(1280.0), px(820.0)));
        assert_eq!(config.min_size, Some(size(px(980.0), px(680.0))));
        assert!(config.transparent_titlebar);
        assert_eq!(
            config.traffic_light_position,
            Some(point(px(14.0), px(17.0)))
        );
        assert!(config.focus);
        assert!(config.show);
        assert_eq!(config.kind, WindowKind::Normal);
        assert!(config.movable);
        assert!(config.resizable);
        assert!(config.minimizable);
        assert_eq!(config.background, WindowBackgroundAppearance::Opaque);
        assert_eq!(config.app_id, None);
        assert_eq!(config.tabbing_identifier, None);
    }

    #[test]
    fn embedded_assets_builder_signature_stays_public_and_typed() {
        let builder: fn(DesktopApp, EmbeddedAssets) -> DesktopApp = DesktopApp::assets;
        let _ = builder;
    }

    #[test]
    fn extended_window_policy_is_retained() {
        let config = WindowConfig::new("Palette", 420.0, 520.0)
            .focused(false)
            .visible(false)
            .kind(WindowKind::PopUp)
            .movable(false)
            .resizable(false)
            .minimizable(false)
            .background(WindowBackgroundAppearance::Blurred)
            .app_id("io.github.gpui-vue.palette")
            .tabbing_identifier("palette");

        assert!(!config.focus);
        assert!(!config.show);
        assert_eq!(config.kind, WindowKind::PopUp);
        assert!(!config.movable);
        assert!(!config.resizable);
        assert!(!config.minimizable);
        assert_eq!(config.background, WindowBackgroundAppearance::Blurred);
        assert_eq!(config.app_id.as_deref(), Some("io.github.gpui-vue.palette"));
        assert_eq!(config.tabbing_identifier.as_deref(), Some("palette"));
    }

    #[test]
    fn setup_builder_signature_stays_public_and_typed() {
        fn install(_app: &mut App) {}

        let builder: fn(DesktopApp, fn(&mut App)) -> DesktopApp = DesktopApp::setup;
        let _ = builder;
        let _ = install;
    }

    #[test]
    fn closures_are_typed_application_plugins() {
        fn assert_plugin(_plugin: impl AppPlugin) {}
        assert_plugin(|_app: &mut App| {});
    }

    #[test]
    fn multi_window_helpers_keep_public_typed_signatures() {
        type RootBuilder = fn(&mut Window, &mut App) -> Entity<TestRoot>;
        type ComponentBuilder = fn(&mut Window, &mut App) -> Entity<TestComponent>;

        let raw: fn(&mut App, WindowConfig, RootBuilder) -> DesktopResult<WindowHandle<TestRoot>> =
            open_window::<TestRoot, RootBuilder>;
        let component: fn(
            &mut App,
            WindowConfig,
            ComponentBuilder,
        ) -> DesktopResult<AnyWindowHandle> =
            open_component_window::<TestComponent, ComponentBuilder>;

        let _: RootBuilder = build_test_root;
        let _: ComponentBuilder = build_test_component;
        let _ = (raw, component);
    }

    #[test]
    fn platform_lifecycle_builders_keep_public_typed_signatures() {
        type OpenUrlsHandler = fn(Vec<String>);
        type ReopenHandler = fn(&mut App);

        let open_urls: fn(DesktopApp, OpenUrlsHandler) -> DesktopApp =
            DesktopApp::on_open_urls::<OpenUrlsHandler>;
        let reopen: fn(DesktopApp, ReopenHandler) -> DesktopApp =
            DesktopApp::on_reopen::<ReopenHandler>;
        let register_scheme: fn(DesktopApp, String) -> DesktopApp =
            DesktopApp::register_url_scheme::<String>;

        let _ = (open_urls, reopen, register_scheme);
    }
}
