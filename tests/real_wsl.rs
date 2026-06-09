//! Integration tests that invoke the real `wsl.exe` and registry on this
//! machine. They are `#[ignore]`d so CI and the default `cargo test` skip them.
//!
//! Run locally with:
//!
//! ```text
//! cargo test --test real_wsl -- --ignored
//! ```

use wsl_manager_tui::wsl::{refresh, RealWslBackend, WslBackend};

#[tokio::test]
#[ignore = "invokes the real wsl.exe on this machine"]
async fn refresh_lists_distros() {
    let backend = RealWslBackend;
    let distros = refresh(&backend).await.expect("refresh should succeed");

    assert!(
        !distros.is_empty(),
        "expected at least one registered distro"
    );
    for distro in &distros {
        assert!(!distro.name.is_empty(), "distro name should not be empty");
        assert!(
            distro.version == 1 || distro.version == 2,
            "unexpected WSL version {} for {}",
            distro.version,
            distro.name
        );
    }
}

#[tokio::test]
#[ignore = "invokes the real wsl.exe on this machine"]
async fn list_online_returns_installable_distros() {
    // Regression for the BOM-less UTF-16LE decode bug: on a non-English locale
    // `wsl --list --online` starts with localized prose, which must still decode
    // and parse into a non-empty list of ASCII install ids.
    let backend = RealWslBackend;
    let online = backend
        .list_online()
        .await
        .expect("list_online should succeed");

    assert!(
        !online.is_empty(),
        "expected at least one installable distro (decode/parse regression)"
    );
    for item in &online {
        assert!(!item.name.is_empty(), "install id should not be empty");
        assert!(
            item.name.is_ascii(),
            "install id should be ASCII, got {:?}",
            item.name
        );
    }
}
