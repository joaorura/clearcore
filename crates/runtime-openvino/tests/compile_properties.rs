//! Verifies compile-time properties and static reshape against the real OpenVINO runtime.
//! Every test skips gracefully when the native library or the stateful models are absent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use realtime_noise_runtime_openvino::{OpenVinoCore, OpenVinoError, is_openvino_available};

fn stateful_model(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/stateful")
        .join(format!("{name}.onnx"));
    path.exists().then_some(path)
}

fn core_and_model() -> Option<(OpenVinoCore, realtime_noise_runtime_openvino::OpenVinoModel)> {
    if !is_openvino_available() {
        eprintln!("OpenVINO runtime not installed; skipping");
        return None;
    }
    let path = stateful_model("df_dec")?;
    let core = OpenVinoCore::new().unwrap();
    let model = core.read_model_from_file(&path, None::<&str>).unwrap();
    Some((core, model))
}

#[test]
fn precision_hint_property_is_parsed_by_the_runtime() {
    let Some((core, model)) = core_and_model() else {
        return;
    };
    // A valid hint compiles.
    core.compile_model_with_properties(
        &model,
        "CPU",
        &[
            ("INFERENCE_PRECISION_HINT", "f32"),
            ("PERFORMANCE_HINT", "LATENCY"),
        ],
    )
    .unwrap();

    // An unknown key in the second pair must be reported by name: proves that both pairs reached
    // the runtime, i.e. the variadic argument count (2 per property) is interpreted correctly.
    let err = core
        .compile_model_with_properties(
            &model,
            "CPU",
            &[
                ("PERFORMANCE_HINT", "LATENCY"),
                ("CLEARCORE_NOT_A_PROPERTY", "x"),
            ],
        )
        .err()
        .expect("unknown property must fail");
    match err {
        OpenVinoError::ModelCompilationFailed(message) => {
            assert!(message.contains("CLEARCORE_NOT_A_PROPERTY"), "{message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn too_many_properties_are_rejected_without_calling_native_code() {
    let Some((core, model)) = core_and_model() else {
        return;
    };
    let props = [("A", "1"), ("B", "2"), ("C", "3"), ("D", "4")];
    let err = core
        .compile_model_with_properties(&model, "CPU", &props)
        .err()
        .unwrap();
    assert!(matches!(err, OpenVinoError::InvalidInput(_)));
}

#[test]
fn static_reshape_makes_dynamic_graph_compilable() {
    let Some((core, mut model)) = core_and_model() else {
        return;
    };
    model.reshape_input("emb", &[1, 1, 512]).unwrap();
    model.reshape_input("c0", &[1, 64, 1, 96]).unwrap();
    core.compile_model_with_properties(&model, "CPU", &[("PERFORMANCE_HINT", "LATENCY")])
        .unwrap();

    let err = model.reshape_input("no_such_input", &[1]).unwrap_err();
    assert!(matches!(err, OpenVinoError::ShapeError(_)), "{err:?}");
}
