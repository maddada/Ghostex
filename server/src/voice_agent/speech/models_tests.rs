//! The local models on disk: the catalog pins every file, a model is ready only once its folder
//! holds what the engine loads, Cancel and Remove clean up, and Whisper's weights are prepared
//! as 8-bit matrices.

use std::fs;

use candle_core::{DType, Device, Tensor};

use super::catalog::{self, Family, MODELS};
use super::models;

fn test_paths(temp: &tempfile::TempDir) -> crate::paths::GxserverPaths {
    crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()))
}

#[test]
fn the_catalog_offers_the_three_chosen_models_pinned_to_one_revision() {
    let ids: Vec<&str> = MODELS.iter().map(|spec| spec.id).collect();
    assert_eq!(
        ids,
        [
            "parakeet-tdt-0.6b-v3",
            "whisper-large-v3-turbo",
            "whisper-base"
        ]
    );
    for spec in &MODELS {
        for file in spec.files {
            let url = spec.url(file);
            assert!(url.starts_with("https://huggingface.co/"), "{url}");
            assert!(
                !url.contains("/main/"),
                "pinned to a commit, not a branch: {url}"
            );
            assert_eq!(file.sha256.len(), 64, "{}", file.name);
            assert!(file.bytes > 0);
        }
    }
    let base = catalog::model("whisper-base").expect("base");
    assert!(
        base.download_bytes() < 300_000_000,
        "Whisper base is the small one"
    );
    assert_eq!(
        catalog::model("parakeet-tdt-0.6b-v3").map(|spec| spec.family),
        Some(Family::Parakeet)
    );
    assert!(catalog::model("whisper-tiny").is_none());
}

#[test]
fn models_live_in_gxservers_data_folder() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = test_paths(&temp);
    let folder = models::folder(&paths);
    assert!(
        folder.ends_with("gxserver/speech-models"),
        "{}",
        folder.display()
    );
    assert!(folder.starts_with(&paths.app_data_dir));
}

#[test]
fn a_model_is_ready_only_when_its_folder_holds_what_the_engine_loads() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = test_paths(&temp);
    let parakeet = catalog::model("parakeet-tdt-0.6b-v3").expect("parakeet");
    let whisper = catalog::model("whisper-base").expect("whisper");
    assert!(!models::is_ready(&paths, parakeet));
    let view = models::view(&paths, parakeet, true);
    assert_eq!(view["state"], "notDownloaded");
    assert_eq!(view["inUse"], true);
    assert_eq!(view["diskBytes"], parakeet.disk_bytes_estimate);

    let dir = models::model_dir(&paths, parakeet.id);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("model.pack"), b"pack").expect("pack");
    assert!(models::is_ready(&paths, parakeet));
    let view = models::view(&paths, parakeet, false);
    assert_eq!(view["state"], "ready");
    assert_eq!(view["diskBytes"], 4, "the real size once on disk");

    let dir = models::model_dir(&paths, whisper.id);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("model-q8_0.gguf"), b"w").expect("weights");
    assert!(
        !models::is_ready(&paths, whisper),
        "config and tokenizer are needed too"
    );
    fs::write(dir.join("config.json"), b"{}").expect("config");
    fs::write(dir.join("tokenizer.json"), b"{}").expect("tokenizer");
    assert!(models::is_ready(&paths, whisper));
    let list = models::list(&paths, whisper.id);
    assert_eq!(list.len(), 3);
    assert_eq!(list[2]["inUse"], true);
    assert_eq!(list[0]["inUse"], false);
}

#[test]
fn remove_deletes_the_model_and_cancel_drops_a_partial_download() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = test_paths(&temp);
    let spec = catalog::model("whisper-base").expect("base");
    let dir = models::model_dir(&paths, spec.id);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("model-q8_0.gguf"), b"w").expect("weights");
    models::delete(&paths, spec).expect("deleted");
    assert!(!dir.exists());
    models::delete(&paths, spec).expect("deleting twice is fine");

    let partial = models::folder(&paths).join(format!("{}.download", spec.id));
    fs::create_dir_all(&partial).expect("partial");
    fs::write(partial.join("model.safetensors.part"), b"half").expect("part");
    models::cancel_download(&paths, spec);
    assert!(!partial.exists());
    assert_eq!(models::view(&paths, spec, false)["state"], "notDownloaded");
}

#[test]
fn sizes_add_up_over_a_folder() {
    let temp = tempfile::tempdir().expect("temp");
    fs::create_dir_all(temp.path().join("a/b")).expect("dirs");
    fs::write(temp.path().join("a/one"), vec![0u8; 10]).expect("one");
    fs::write(temp.path().join("a/b/two"), vec![0u8; 5]).expect("two");
    assert_eq!(models::disk_bytes(&temp.path().join("a")), 15);
    assert_eq!(models::disk_bytes(&temp.path().join("missing")), 0);
}

#[test]
fn a_file_is_checked_by_its_sha256() {
    let temp = tempfile::tempdir().expect("temp");
    let file = temp.path().join("f");
    fs::write(&file, b"abc").expect("write");
    assert_eq!(
        models::sha256_file(&file).expect("hash"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn whisper_weights_are_prepared_as_eight_bit_matrices() {
    let temp = tempfile::tempdir().expect("temp");
    let device = Device::Cpu;
    let matrix = Tensor::randn(0f32, 1.0, (8, 64), &device).expect("matrix");
    let bias = Tensor::zeros(8, DType::F32, &device).expect("bias");
    let conv = Tensor::randn(0f32, 1.0, (8, 4, 3), &device).expect("conv");
    let half = matrix.to_dtype(DType::F16).expect("f16");
    let tensors = [
        ("model.decoder.fc1.weight".to_string(), half),
        ("model.decoder.fc1.bias".to_string(), bias),
        ("model.encoder.conv1.weight".to_string(), conv),
    ]
    .into_iter()
    .collect::<std::collections::HashMap<_, _>>();
    candle_core::safetensors::save(&tensors, temp.path().join("model.safetensors")).expect("save");

    super::whisper::prepare(temp.path()).expect("prepared");
    assert!(
        !temp.path().join("model.safetensors").exists(),
        "the f16 source goes"
    );
    let mut file = fs::File::open(temp.path().join("model-q8_0.gguf")).expect("gguf");
    let content = candle_core::quantized::gguf_file::Content::read(&mut file).expect("read");
    let kind = |name: &str| content.tensor_infos[name].ggml_dtype;
    assert_eq!(
        kind("model.decoder.fc1.weight"),
        candle_core::quantized::GgmlDType::Q8_0
    );
    assert_eq!(
        kind("model.decoder.fc1.bias"),
        candle_core::quantized::GgmlDType::F32
    );
    assert_eq!(
        kind("model.encoder.conv1.weight"),
        candle_core::quantized::GgmlDType::F32
    );
}
