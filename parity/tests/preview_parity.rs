//! Visual parity driver invoking `water preview` across `waterui-apple`
//! (`--platform ios`) and `hydrolysis` (`--theme glass`) and reporting FLIP
//! perceptual metrics plus an HTML gallery through `waterui-testing`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use waterui_testing::{
    CaseReport, FlipThresholds, GalleryReport, Snapshot, StageComparison, StageReport, SuiteReport,
    write_gallery_artifacts,
};

const WATER_BIN_ENV: &str = "WATER_BIN";
const ARTIFACTS_ENV: &str = "WATERUI_TEST_ARTIFACTS_DIR";
const ASSERT_PARITY_ENV: &str = "WATERUI_ASSERT_PARITY";
const PREVIEW_SCALE: f32 = 2.0;

struct ParityTarget {
    preview_fn: &'static str,
    width: u32,
    height: u32,
}

const TARGETS: &[ParityTarget] = &[
    ParityTarget {
        preview_fn: "button_bordered",
        width: 220,
        height: 80,
    },
    ParityTarget {
        preview_fn: "button_glass",
        width: 220,
        height: 80,
    },
    ParityTarget {
        preview_fn: "button_prominent",
        width: 220,
        height: 80,
    },
    ParityTarget {
        preview_fn: "button_glass_backdrop",
        width: 260,
        height: 96,
    },
    ParityTarget {
        preview_fn: "button_prominent_backdrop",
        width: 260,
        height: 96,
    },
    ParityTarget {
        preview_fn: "toggle_switch",
        width: 220,
        height: 80,
    },
    ParityTarget {
        preview_fn: "toggle_checkbox",
        width: 220,
        height: 80,
    },
    ParityTarget {
        preview_fn: "slider_control",
        width: 320,
        height: 80,
    },
    ParityTarget {
        preview_fn: "stepper_control",
        width: 260,
        height: 80,
    },
    ParityTarget {
        preview_fn: "text_field",
        width: 300,
        height: 80,
    },
    ParityTarget {
        preview_fn: "picker_segmented",
        width: 320,
        height: 80,
    },
    ParityTarget {
        preview_fn: "progress_linear",
        width: 300,
        height: 80,
    },
    ParityTarget {
        preview_fn: "progress_circular",
        width: 160,
        height: 80,
    },
    ParityTarget {
        preview_fn: "tabs",
        width: 400,
        height: 260,
    },
    ParityTarget {
        preview_fn: "navigation",
        width: 400,
        height: 260,
    },
];

fn artifact_root() -> PathBuf {
    env::var_os(ARTIFACTS_ENV).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/verification/preview_parity"),
        PathBuf::from,
    )
}

fn water_bin() -> PathBuf {
    env::var_os(WATER_BIN_ENV).map_or_else(|| PathBuf::from("water"), PathBuf::from)
}

fn run_preview(water: &Path, project_dir: &Path, args: &[&str], output: &Path) {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("create preview artifact directory");
    }
    let status = Command::new(water)
        .arg("preview")
        .args(args)
        .arg("--path")
        .arg(project_dir)
        .arg("--scale")
        .arg(PREVIEW_SCALE.to_string())
        .arg("--output")
        .arg(output)
        .status()
        .unwrap_or_else(|error| panic!("failed to invoke {}: {error}", water.display()));
    assert!(
        status.success(),
        "{} preview {:?} failed with status {status}",
        water.display(),
        args
    );
}

struct CaseArtifacts<'a> {
    water: &'a Path,
    project_dir: &'a Path,
    root: &'a Path,
    thresholds: &'a FlipThresholds,
}

fn capture_pair(
    ctx: &CaseArtifacts<'_>,
    target: &ParityTarget,
    scheme: &str,
    reference_path: &Path,
    candidate_path: &Path,
) -> (Snapshot, Snapshot) {
    let frame = format!("{}x{}", target.width, target.height);
    run_preview(
        ctx.water,
        ctx.project_dir,
        &[
            target.preview_fn,
            "--platform",
            "ios",
            "--color-scheme",
            scheme,
            "--frame",
            &frame,
        ],
        reference_path,
    );
    run_preview(
        ctx.water,
        ctx.project_dir,
        &[
            target.preview_fn,
            "--platform",
            "macos",
            "--backend",
            "hydrolysis",
            "--theme",
            "glass",
            "--color-scheme",
            scheme,
            "--frame",
            &frame,
        ],
        candidate_path,
    );
    let reference = Snapshot::load_png(reference_path).expect("load reference PNG");
    let candidate = Snapshot::load_png(candidate_path).expect("load candidate PNG");
    assert_eq!(
        (reference.width(), reference.height()),
        (candidate.width(), candidate.height()),
        "{}_{scheme}: reference and candidate dimensions differ",
        target.preview_fn,
    );
    assert!(
        reference.width() > 0 && reference.height() > 0,
        "{}_{scheme}: zero-sized capture",
        target.preview_fn,
    );
    (reference, candidate)
}

fn run_case(
    ctx: &CaseArtifacts<'_>,
    target: &ParityTarget,
    scheme: &str,
) -> (CaseReport, Option<String>) {
    let case_name = format!("{}_{scheme}", target.preview_fn);
    let case_dir = ctx.root.join("controls").join(&case_name);
    let reference_rel = format!("controls/{case_name}/00_idle__reference.png");
    let candidate_rel = format!("controls/{case_name}/00_idle.png");
    let heatmap_rel = format!("controls/{case_name}/00_idle__flip_heatmap.png");
    let reference_path = case_dir.join("00_idle__reference.png");
    let candidate_path = case_dir.join("00_idle.png");
    let heatmap_path = case_dir.join("00_idle__flip_heatmap.png");

    let (reference, candidate) =
        capture_pair(ctx, target, scheme, &reference_path, &candidate_path);

    let ref_blank = reference.analyze_blank();
    assert!(
        !ref_blank.is_blank(),
        "{case_name}: waterui-apple reference preview rendered blank: {ref_blank:?}"
    );
    let blank = candidate.analyze_blank();
    assert!(
        !blank.is_blank(),
        "{case_name}: hydrolysis-glass preview rendered blank: {blank:?}"
    );

    let flip = reference
        .compare_flip(&candidate)
        .expect("FLIP comparison succeeds");
    let metrics = flip.metrics();
    flip.magma_heatmap_snapshot()
        .save_png(&heatmap_path)
        .expect("save FLIP heatmap PNG");

    let failures = ctx.thresholds.evaluate(&metrics);
    let passed = failures.is_empty();
    let failure_line = (!passed).then(|| format!("{case_name}: {}", failures.join("; ")));

    let case = CaseReport {
        name: case_name,
        appearance: scheme.to_string(),
        viewport_width: target.width,
        viewport_height: target.height,
        scale_factor: f64::from(PREVIEW_SCALE),
        stages: vec![StageReport {
            name: "00_idle".to_string(),
            image_path: candidate_rel,
            candidate_label: Some("hydrolysis-glass".to_string()),
            width: candidate.width(),
            height: candidate.height(),
            blank,
            comparison: Some(StageComparison {
                reference_path: reference_rel,
                reference_label: Some("waterui-apple (ios)".to_string()),
                heatmap_path: Some(heatmap_rel),
                metrics,
            }),
            passed,
            failures,
        }],
    };
    (case, failure_line)
}

#[test]
fn ios_vs_glass_preview_parity() {
    let water = water_bin();
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = artifact_root();
    let thresholds = FlipThresholds::cross_backend_parity();
    let ctx = CaseArtifacts {
        water: &water,
        project_dir,
        root: &root,
        thresholds: &thresholds,
    };

    let mut cases = Vec::with_capacity(TARGETS.len() * 2);
    let mut threshold_failures = Vec::new();

    for target in TARGETS {
        for scheme in ["light", "dark"] {
            let (case, failure) = run_case(&ctx, target, scheme);
            cases.push(case);
            if let Some(line) = failure {
                threshold_failures.push(line);
            }
        }
    }

    let report = GalleryReport::new(
        "waterui-apple (ios) vs hydrolysis-glass Preview Parity",
        vec![SuiteReport {
            name: "controls".to_string(),
            cases,
        }],
    );
    let paths = write_gallery_artifacts(&root, &report).expect("write parity gallery artifacts");
    assert!(paths.report_json.is_file(), "missing report.json");
    assert!(paths.index_html.is_file(), "missing index.html");

    if env::var_os(ASSERT_PARITY_ENV).is_some_and(|v| v == "1") {
        assert!(
            threshold_failures.is_empty(),
            "FLIP parity threshold violations:\n{}",
            threshold_failures.join("\n")
        );
    }
}
