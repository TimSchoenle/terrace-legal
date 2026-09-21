//! Editing a mounted body file through the reload supervisor.
#![cfg(feature = "testing")]

use std::time::Duration;

use terrace_config::reload::{Debounce, run_with};
use terrace_config::testing::{Rebuilds, ServiceError};
use terrace_legal::Catalog;
use terrace_legal::testing::{HostConfig, LegalFixture};
use tokio_util::sync::CancellationToken;

/// A text edit applies without a restart, and an edit that leaves the configuration invalid is
/// refused while the previous documents keep serving.
///
/// The reload closure validates the catalog the way a host does, so a refused edit is a failed
/// reload and the supervisor leaves the running service alone.
#[test]
fn an_edit_rebuilds_once_and_a_broken_edit_keeps_the_previous_catalog() {
    LegalFixture::run(|jail| {
        let file = LegalFixture::body_file(jail, "terms", "en", "# v1")?;
        let boot = jail.load_watched::<HostConfig>()?;
        let loader = jail.terrace();
        let rebuilds: Rebuilds = Rebuilds::new().patience(Duration::from_secs(20));

        jail.block_on(async {
            let shutdown = CancellationToken::new();
            let driver = rebuilds.clone();
            let stop = shutdown.clone();
            tokio::spawn(async move {
                driver.wait_for(1).await;

                std::fs::write(&file, "# v2").expect("edit");
                driver.wait_for(2).await;
                driver.stays_at(2, Duration::from_millis(400)).await;

                // Blank text is refused by the catalog, so this reload fails.
                std::fs::write(&file, "   \n").expect("edit");
                driver.stays_at(2, Duration::from_millis(800)).await;

                // A later valid edit still applies: the refusal did not wedge the supervisor.
                std::fs::write(&file, "# v3").expect("edit");
                driver.wait_for(3).await;
                stop.cancel();
            });

            run_with(
                (boot.value, boot.sources),
                &shutdown,
                move || {
                    let loaded = loader
                        .load_watched::<HostConfig>()
                        .map_err(ServiceError::from)?;
                    Catalog::build(&loaded.value.legal).map_err(|issues| {
                        ServiceError::Runtime(issues.with_prefix("legal").to_string())
                    })?;
                    Ok((loaded.value, loaded.sources))
                },
                rebuilds.serving(|config: &HostConfig| {
                    config.legal.documents["terms"].body["en"].clone()
                }),
                Debounce(Duration::from_millis(50)),
            )
            .await
            .expect("the supervisor returns when shutdown is cancelled");
        });

        assert_eq!(rebuilds.seen(), ["# v1", "# v2", "# v3"]);
        Ok(())
    });
}
