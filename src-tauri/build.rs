use std::fs;
use std::process::Command;

/// Identité de l'application (nom, identifiant…) : source unique, `identity.json`.
/// Exposée au code Rust via `env!("APP_NAME")` etc. ; `tauri.conf.json` doit concorder.
fn identity() {
    println!("cargo:rerun-if-changed=../identity.json");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let raw = fs::read_to_string("../identity.json").expect("identity.json introuvable");
    let id: serde_json::Value = serde_json::from_str(&raw).expect("identity.json invalide");
    let get = |k: &str| id[k].as_str().unwrap_or_else(|| panic!("identity.json : champ « {k} » manquant")).to_string();
    let (name, slug, app_id) = (get("name"), get("slug"), get("id"));
    println!("cargo:rustc-env=APP_NAME={name}");
    println!("cargo:rustc-env=APP_SLUG={slug}");
    println!("cargo:rustc-env=APP_ID={app_id}");
    println!("cargo:rustc-env=APP_REPOSITORY={}", get("repository"));
    let migrate: Vec<String> = id["migrateFrom"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    println!("cargo:rustc-env=APP_MIGRATE_FROM={}", migrate.join(";"));

    let conf: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("tauri.conf.json").expect("tauri.conf.json")).expect("tauri.conf.json invalide");
    for (field, expected) in [("productName", &name), ("identifier", &app_id), ("mainBinaryName", &slug)] {
        assert_eq!(
            conf[field].as_str(),
            Some(expected.as_str()),
            "tauri.conf.json : « {field} » ne correspond pas à identity.json (les deux fichiers doivent être modifiés ensemble)"
        );
    }
}

/// Numéro de build (nombre de commits) et commit, affichés dans « À propos ».
/// La CI les écrit dans `build-info` (deux lignes) ; à défaut on interroge git.
fn build_info() {
    println!("cargo:rerun-if-changed=../build-info");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
    let file = fs::read_to_string("../build-info").unwrap_or_default();
    let mut lines = file.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from);
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir("..")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let build = lines.next().or_else(|| git(&["rev-list", "--count", "HEAD"])).unwrap_or_else(|| "dev".into());
    let commit = lines.next().or_else(|| git(&["rev-parse", "--short", "HEAD"])).unwrap_or_default();
    println!("cargo:rustc-env=APP_BUILD={build}");
    println!("cargo:rustc-env=APP_COMMIT={commit}");
}

fn main() {
    identity();
    build_info();
    // Déclarer explicitement les commandes : chacune doit alors être autorisée
    // par une capability (voir capabilities/main.json). Rien n'est exposé par défaut.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "app_info",
            "set_language",
            "update_check",
            "update_open",
            "update_install",
            "update_restart",
            "vault_status",
            "vault_create",
            "vault_unlock",
            "vault_lock",
            "vault_touch",
            "vault_change_password",
            "vault_export",
            "vault_import_pick",
            "vault_import_preview",
            "vault_import_apply",
            "vault_import_cancel",
            "hosts_list",
            "host_save",
            "host_delete",
            "identities_list",
            "identity_save",
            "identity_delete",
            "identity_copy_password",
            "identity_generate_key",
            "settings_get",
            "settings_save",
            "ssh_open",
            "ssh_write",
            "ssh_paste",
            "ssh_resize",
            "ssh_close",
            "ssh_prompt_answer",
            "known_hosts_list",
            "known_host_delete",
            "sftp_open",
            "sftp_close",
            "sftp_list",
            "sftp_realpath",
            "sftp_mkdir",
            "sftp_rename",
            "sftp_delete",
            "app_reset",
            "sftp_read_text",
            "sftp_write_text",
            "edits_subscribe",
            "sftp_open_external",
            "edit_external_overwrite",
            "edit_external_stop",
            "sftp_stat",
            "sftp_chmod",
            "local_stat",
            "local_chmod",
            "local_home",
            "local_list",
            "local_realpath",
            "local_mkdir",
            "local_rename",
            "local_trash",
            "transfers_subscribe",
            "transfer_cancel",
            "sftp_upload",
            "sftp_download",
            "oidc_status",
            "oidc_redirect_uri",
            "oidc_enroll",
            "oidc_disable",
            "oidc_unlock",
            "oidc_cancel",
            "oidc_device_start",
            "oidc_device_wait",
            "fido2_list",
            "fido2_detect",
            "fido2_set_pin",
            "fido2_enroll",
            "fido2_remove",
            "fido2_unlock",
        ]),
    ))
    .expect("échec de tauri-build");
}
