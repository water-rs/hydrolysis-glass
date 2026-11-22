//! Per-component offscreen FLIP visual parity harness against `waterui-apple` on
//! macOS 27.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    macos_parity::run();
}

#[cfg(target_os = "macos")]
mod macos_parity {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use cocoa_ui::ColorScheme as CocoaColorScheme;
    use cocoa_ui::appkit::{ActivationPolicy, Application};
    use hydrolysis::{FontFamilyResolution, HeadlessRuntime, OutputAlpha};
    use hydrolysis_glass::Glass;
    use libtest_mimic::{Arguments, Failed, Trial};
    use waterui::component::progress::progress;
    use waterui::form::picker::{PickerStyle, picker};
    use waterui::graphics::color::ColorScheme;
    use waterui::layout::Size;
    use waterui::layout::padding::EdgeInsets;
    use waterui::layout::stack::zstack;
    use waterui::navigation::{NavigationStack, NavigationView, Tab, Tabs};
    use waterui::reactive::{Binding, SignalExt, binding};
    use waterui::text::text;
    use waterui::theme::{color::Background, install_color_scheme, installed_color_scheme};
    use waterui::{Environment, Str, View, ViewExt};
    use waterui_apple::MainThreadMarker;
    use waterui_apple::native_test_support::view_renderer::{AppleOffscreenRenderer, GpuRuntime};
    use waterui_apple::native_test_support::{
        block_on_main, gpu_surface, install_gpu, refresh_theme, window_environment,
    };
    use waterui_apple::objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    };
    use waterui_capture::view_renderer::{RenderError, RenderRequest, RenderResult, ViewRenderer};
    use waterui_conformance::{
        FlipThresholds, ParityCase, ParityConfig, ParityRunOutput, ParityRunner, ParitySuite,
    };
    use waterui_controls::button::button;
    use waterui_controls::slider::slider;
    use waterui_controls::stepper::stepper;
    use waterui_controls::toggle::{ToggleStyle, toggle};
    use waterui_controls::{ButtonStyle, field};
    use waterui_core::Signal as _;
    use waterui_core::handler::ViewBuilder;

    /// Virtual frame interval matching `tests/common/mod.rs` so Hydrolysis
    /// transitions settle deterministically before snapshot readback.
    const VIRTUAL_FRAME: Duration = Duration::from_millis(16);
    const SETTLE_FRAMES: usize = 120;
    const CONTENT_DEADLINE: Duration = Duration::from_secs(10);
    const SUITE_TIMEOUT_SECS: f64 = 120.0;
    const ARTIFACTS_ENV: &str = "WATERUI_TEST_ARTIFACTS_DIR";
    const ASSERT_PARITY_ENV: &str = "WATERUI_ASSERT_APPLE_PARITY";

    struct OnceViewBuilder<V>(Cell<Option<V>>);

    impl<V> OnceViewBuilder<V> {
        const fn new(view: V) -> Self {
            Self(Cell::new(Some(view)))
        }
    }

    impl<V: View> ViewBuilder for OnceViewBuilder<V> {
        type Output = V;

        fn build(&self) -> Self::Output {
            self.0
                .take()
                .expect("parity headless view builder invoked more than once")
        }
    }

    struct AppleParityRenderer<F> {
        mtm: MainThreadMarker,
        offscreen: AppleOffscreenRenderer,
        base_env: Environment,
        refresh: F,
        shared_scale: Rc<Cell<f32>>,
    }

    impl<F> ViewRenderer for AppleParityRenderer<F>
    where
        F: Fn(CocoaColorScheme) + 'static,
    {
        #[expect(
            clippy::future_not_send,
            reason = "offscreen native rendering executes on the macOS main thread"
        )]
        async fn render_to_rgba<V: View>(
            &self,
            view: V,
            env: &Environment,
            request: RenderRequest,
        ) -> Result<RenderResult, RenderError> {
            let scheme = installed_color_scheme(env).map_or(ColorScheme::Light, |s| s.snapshot());
            let (cocoa_scheme, appearance_name) = match scheme {
                ColorScheme::Light => (
                    CocoaColorScheme::Light,
                    // SAFETY: `NSAppearanceNameAqua` is an immutable system string constant.
                    unsafe { NSAppearanceNameAqua },
                ),
                ColorScheme::Dark => (
                    CocoaColorScheme::Dark,
                    // SAFETY: `NSAppearanceNameDarkAqua` is an immutable system string constant.
                    unsafe { NSAppearanceNameDarkAqua },
                ),
            };
            let appearance = NSAppearance::appearanceNamed(appearance_name)
                .expect("system Aqua appearance must exist on macOS");
            NSApplication::sharedApplication(self.mtm).setAppearance(Some(&appearance));
            (self.refresh)(cocoa_scheme);

            let result = self
                .offscreen
                .render_to_rgba(view, &self.base_env, request)
                .await?;

            let width = result.width();
            let height = result.height();
            #[expect(
                clippy::cast_precision_loss,
                reason = "rendered parity raster dimensions are small screen pixel counts"
            )]
            let scale = if request.size().width > 0.0 {
                (width as f32 / request.size().width).round().max(1.0)
            } else {
                1.0
            };
            self.shared_scale.set(scale);

            let straight = unpremultiply_rgba8(result.rgba_data());
            Ok(RenderResult::new(straight, width, height))
        }
    }

    struct GlassParityRenderer {
        shared_scale: Rc<Cell<f32>>,
    }

    impl GlassParityRenderer {
        fn render_sync<V: View>(
            &self,
            view: V,
            env: &Environment,
            request: RenderRequest,
        ) -> Result<RenderResult, RenderError> {
            let scheme = installed_color_scheme(env).map_or(ColorScheme::Light, |s| s.snapshot());
            let style = Glass::for_scheme(scheme);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "parity case viewport dimensions are positive integers"
            )]
            let (width, height) = (
                request.size().width.max(1.0).round() as u32,
                request.size().height.max(1.0).round() as u32,
            );
            let scale = f64::from(self.shared_scale.get());

            let mut runtime = HeadlessRuntime::new(
                env.clone(),
                OnceViewBuilder::new(view),
                width,
                height,
                style,
                FontFamilyResolution::Lenient,
            )
            .map_err(|err| RenderError::Host(format!("headless painter unavailable: {err}")))?
            .with_scale_factor(scale);

            let mut at = Instant::now();
            for _ in 0..SETTLE_FRAMES {
                at += VIRTUAL_FRAME;
                let _ = runtime.pump_at(false, at);
                if runtime.is_settled() {
                    break;
                }
            }
            at += VIRTUAL_FRAME;
            let snapshot = runtime.pump_at(true, at).into_snapshot().ok_or_else(|| {
                RenderError::Host(String::from("headless capture produced no frame"))
            })?;
            assert_eq!(
                snapshot.output_alpha(),
                OutputAlpha::Straight,
                "headless Glass surface must output straight RGBA8"
            );

            Ok(RenderResult::new(
                snapshot.rgba8().to_vec(),
                snapshot.width(),
                snapshot.height(),
            ))
        }
    }

    impl ViewRenderer for GlassParityRenderer {
        fn render_to_rgba<V: View>(
            &self,
            view: V,
            env: &Environment,
            request: RenderRequest,
        ) -> impl std::future::Future<Output = Result<RenderResult, RenderError>> {
            std::future::ready(self.render_sync(view, env, request))
        }
    }

    /// Converts premultiplied RGBA8 pixels to straight-alpha RGBA8 pixels to
    /// match `HeadlessSnapshot` (`OutputAlpha::Straight`) and FLIP input.
    fn unpremultiply_rgba8(pixels: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(pixels.len());
        for px in pixels.as_chunks::<4>().0 {
            let a = px[3];
            if a == 0 || a == u8::MAX {
                out.extend_from_slice(px);
            } else {
                let a16 = u16::from(a);
                let unpremul = |c: u8| -> u8 {
                    let val = (u16::from(c) * 255 + a16 / 2) / a16;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "clamped to u8::MAX before truncating cast"
                    )]
                    let byte = val.min(u16::from(u8::MAX)) as u8;
                    byte
                };
                out.extend_from_slice(&[unpremul(px[0]), unpremul(px[1]), unpremul(px[2]), a]);
            }
        }
        out
    }

    fn artifact_dir() -> PathBuf {
        std::env::var_os(ARTIFACTS_ENV).map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/verification/apple-parity"),
            PathBuf::from,
        )
    }

    /// Wraps a control over the theme background at fixed point dimensions so
    /// both backends render onto identical viewport extents.
    fn framed<V: View>(view: V, width: f32, height: f32) -> impl View {
        zstack((Background, view.padding_with(EdgeInsets::all(12.0)))).size(width, height)
    }

    /// Wraps a container component (`Tabs` / `NavigationStack`) that fills its
    /// proposed bounds over the theme background.
    fn container_framed<V: View>(view: V, width: f32, height: f32) -> impl View {
        zstack((Background, view)).size(width, height)
    }

    fn add_light_dark<V, F>(
        suite: &mut ParitySuite,
        base_name: &str,
        width: f32,
        height: f32,
        factory: F,
    ) where
        V: View,
        F: Fn() -> V + Clone + 'static,
    {
        for (scheme, label) in [(ColorScheme::Light, "light"), (ColorScheme::Dark, "dark")] {
            let build = factory.clone();
            let case = ParityCase::new(
                format!("{base_name}_{label}"),
                Size::new(width, height),
                build,
            )
            .appearance(label)
            .environment(move |env| {
                install_color_scheme(env, binding(scheme).computed());
            });
            suite.add_case(case);
        }
    }

    fn build_suite() -> ParitySuite {
        let mut suite = ParitySuite::new("controls");

        add_light_dark(&mut suite, "button_bordered", 220.0, 80.0, || {
            framed(button("Continue").style(ButtonStyle::Bordered), 220.0, 80.0)
        });
        add_light_dark(&mut suite, "button_glass", 220.0, 80.0, || {
            framed(button("Continue").style(ButtonStyle::Glass), 220.0, 80.0)
        });
        add_light_dark(&mut suite, "button_prominent", 220.0, 80.0, || {
            framed(
                button("Continue").style(ButtonStyle::GlassProminent),
                220.0,
                80.0,
            )
        });
        add_light_dark(&mut suite, "toggle_switch", 220.0, 80.0, || {
            let on = Binding::bool(true);
            framed(toggle("Wi-Fi", &on).style(ToggleStyle::Switch), 220.0, 80.0)
        });
        add_light_dark(&mut suite, "toggle_checkbox", 220.0, 80.0, || {
            let on = Binding::bool(true);
            framed(
                toggle("Wi-Fi", &on).style(ToggleStyle::Checkbox),
                220.0,
                80.0,
            )
        });
        add_light_dark(&mut suite, "slider", 320.0, 80.0, || {
            let val = Binding::f64(0.55);
            framed(slider("Volume", &val).range(0.0..=1.0), 320.0, 80.0)
        });
        add_light_dark(&mut suite, "stepper", 260.0, 80.0, || {
            let count = Binding::i32(5);
            framed(stepper("Copies", &count).range(0..=10), 260.0, 80.0)
        });
        add_light_dark(&mut suite, "text_field", 300.0, 80.0, || {
            let value = Binding::container(Str::from("Ada Lovelace"));
            framed(field("Name", &value).prompt("Your name"), 300.0, 80.0)
        });
        add_light_dark(&mut suite, "picker_segmented", 320.0, 80.0, || {
            let selection = Binding::container(1_i32);
            let items: Vec<_> = ["Day", "Week", "Month"]
                .into_iter()
                .zip(0_i32..)
                .map(|(label, tag)| text(label).tag(tag))
                .collect();
            framed(
                picker("View", items, &selection).style(PickerStyle::Segmented),
                320.0,
                80.0,
            )
        });
        add_light_dark(&mut suite, "progress_linear", 300.0, 80.0, || {
            framed(progress(0.65), 300.0, 80.0)
        });
        add_light_dark(&mut suite, "progress_circular", 160.0, 80.0, || {
            framed(progress(0.65).circular(), 160.0, 80.0)
        });
        add_light_dark(&mut suite, "tabs", 400.0, 260.0, || {
            let selection = Binding::container(0_usize);
            container_framed(
                Tabs::new(
                    &selection,
                    (
                        Tab::new("Home", || NavigationView::new("Home", text("Home"))),
                        Tab::new("Search", || NavigationView::new("Search", text("Search"))),
                    ),
                ),
                400.0,
                260.0,
            )
        });
        add_light_dark(&mut suite, "navigation", 400.0, 260.0, || {
            container_framed(
                NavigationStack::new(NavigationView::new("Root", text("Root"))),
                400.0,
                260.0,
            )
        });

        suite
    }

    fn verify_outputs(output: &ParityRunOutput, artifact_root: &Path) -> Result<(), Failed> {
        let artifacts = output
            .artifacts
            .as_ref()
            .ok_or("parity runner did not emit gallery artifacts")?;
        if !artifacts.report_json.is_file() {
            return Err(
                format!("missing report.json at {}", artifacts.report_json.display()).into(),
            );
        }
        if !artifacts.index_html.is_file() {
            return Err(format!("missing index.html at {}", artifacts.index_html.display()).into());
        }

        println!(
            "\n=== Apple Parity Baseline ({}) ===",
            artifact_root.display()
        );
        println!(
            "{:<28} | {:>9} | {:>8} | {:>12} | {:>9}",
            "Case", "FLIP Mean", "FLIP P95", "MaxBlkMean", "MaxLocal"
        );
        println!("{:-<76}", "");
        for outcome in &output.outcomes {
            let m = &outcome.metrics;
            println!(
                "{:<28} | {:>9.4} | {:>8.4} | {:>12.4} | {:>9.4}",
                outcome.case_name,
                m.flip_mean,
                m.flip_p95,
                m.max_block_flip_mean,
                m.max_local_error,
            );
            assert_eq!(
                (
                    outcome.reference_image.width(),
                    outcome.reference_image.height()
                ),
                (
                    outcome.candidate_image.width(),
                    outcome.candidate_image.height()
                ),
                "{}: reference and candidate dimensions differ",
                outcome.case_name
            );
            assert!(
                outcome.heatmap_image.is_some(),
                "{}: heatmap image was not generated",
                outcome.case_name
            );
            assert!(
                outcome.stage_report.width > 0 && outcome.stage_report.height > 0,
                "{}: stage report recorded zero dimensions",
                outcome.case_name
            );
        }

        if std::env::var_os(ASSERT_PARITY_ENV).is_some_and(|v| v == "1") {
            output
                .clone()
                .into_result()
                .map_err(|err| format!("FLIP parity threshold check failed: {err}"))?;
        }
        Ok(())
    }

    /// Runs the `libtest-mimic` suite on the process main thread with the
    /// `Prohibited` activation policy so zero visible windows or Dock icons are
    /// created.
    pub fn run() {
        let mut args = Arguments::from_args();
        args.test_threads = Some(1);

        let trials = vec![Trial::test("apple_parity_suite", run_parity_trial)];
        libtest_mimic::run(&args, trials).exit();
    }

    fn run_parity_trial() -> Result<(), Failed> {
        let mtm = MainThreadMarker::new().ok_or("apple_parity must run on the main thread")?;
        gpu_surface::initialize_process();

        let app = Application::shared(mtm);
        if app.activation_policy() != ActivationPolicy::Prohibited
            && !app.set_activation_policy(ActivationPolicy::Prohibited)
        {
            return Err("AppKit refused Prohibited activation policy".into());
        }
        assert_eq!(
            app.activation_policy(),
            ActivationPolicy::Prohibited,
            "parity runner must remain Prohibited"
        );
        let ns_app = NSApplication::sharedApplication(mtm);
        assert!(
            !ns_app.isActive(),
            "Prohibited parity runner must never steal focus"
        );

        let (mut apple_env, theme) = window_environment(mtm);
        let gpu_runtime = block_on_main(30.0, GpuRuntime::new())
            .map_err(|err| format!("failed to initialize Apple parity GPU runtime: {err:?}"))?;
        install_gpu(&mut apple_env, gpu_runtime);

        let offscreen = AppleOffscreenRenderer::new(mtm);
        let shared_scale = Rc::new(Cell::new(2.0_f32));
        let apple_renderer = AppleParityRenderer {
            mtm,
            offscreen,
            base_env: apple_env,
            refresh: move |scheme| refresh_theme(&theme, scheme),
            shared_scale: Rc::clone(&shared_scale),
        };
        let glass_renderer = GlassParityRenderer { shared_scale };

        let artifact_root = artifact_dir();
        let suite = build_suite();
        let config = ParityConfig::default()
            .title("waterui-apple vs hydrolysis-glass Parity (macOS 27)")
            .backend_labels("waterui-apple", "hydrolysis-glass")
            .artifact_root(&artifact_root)
            .default_thresholds(FlipThresholds::default())
            .content_deadline(CONTENT_DEADLINE);
        let runner = ParityRunner::new(config);

        let output = block_on_main(
            SUITE_TIMEOUT_SECS,
            runner.run_suite_async(&suite, &apple_renderer, &glass_renderer),
        )
        .map_err(|err| format!("apple parity suite failed: {err}"))?;

        verify_outputs(&output, &artifact_root)?;
        Ok(())
    }
}
