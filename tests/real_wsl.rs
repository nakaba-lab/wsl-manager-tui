//! Integration tests that invoke the real `wsl.exe` and registry on this
//! machine. They are `#[ignore]`d so CI and the default `cargo test` skip them.
//!
//! Run locally with:
//!
//! ```text
//! cargo test --test real_wsl -- --ignored
//! ```

use wsl_manager_tui::wsl::{refresh, RealWslBackend};

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
