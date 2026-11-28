#[cfg(unix)]
#[test]
fn local_cookie_expiry_preserves_persistent_dates_across_clock_changes()
-> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "NUMERA_COOKIE_EXPIRY_DST_CHILD";
    // Observed through normalize() in the final Python source c03deb8f.
    // Each child starts with its own TZ so no concurrent test changes process state.
    let cases: &[(&str, &[(&str, f64)])] = &[
        (
            "America/New_York",
            &[
                ("2026-03-08T01:30:00", 1772951400.0),
                ("2026-03-08T02:00:00", 1772953200.0),
                ("2026-03-08T02:30:00", 1772955000.0),
                ("2026-03-08T03:30:00", 1772955000.0),
                ("2026-11-01T01:30:00", 1793511000.0),
                ("2026-11-01T01:59:59.5", 1793512799.5),
                ("2026-11-01T02:00:00", 1793516400.0),
            ],
        ),
        (
            "Europe/Berlin",
            &[
                ("2026-03-29T01:30:00", 1774744200.0),
                ("2026-03-29T02:00:00", 1774746000.0),
                ("2026-03-29T02:30:00", 1774747800.0),
                ("2026-03-29T03:30:00", 1774747800.0),
                ("2026-10-25T02:30:00", 1792888200.0),
                ("2026-10-25T02:59:59.5", 1792889999.5),
                ("2026-10-25T03:00:00", 1792893600.0),
            ],
        ),
        (
            "Australia/Lord_Howe",
            &[
                ("2026-10-04T01:45:00", 1791040500.0),
                ("2026-10-04T02:15:00", 1791042300.0),
                ("2026-10-04T02:45:00", 1791042300.0),
            ],
        ),
        (
            "Pacific/Apia",
            &[
                ("2011-12-29T12:00:00", 1325196000.0),
                ("2011-12-30T12:00:00", 1325282400.0),
                ("2011-12-31T12:00:00", 1325282400.0),
            ],
        ),
    ];
    if let Ok(zone) = std::env::var(CHILD) {
        let (_, observations) = cases
            .iter()
            .find(|(name, _)| *name == zone)
            .ok_or("Unknown isolated timezone fixture")?;
        for (expiry, expected) in *observations {
            let cookie = numera_core::browser::cookie_normalize::normalize(&serde_json::json!({
                "name":"fixture", "domain":".fixture.test", "expires":expiry
            }))?
            .ok_or("Valid cookie was discarded")?;
            assert_eq!(
                cookie["expires"].as_f64(),
                Some(*expected),
                "Persistent cookie expiry changed in {zone} at {expiry}"
            );
        }
        return Ok(());
    }
    for (zone, _) in cases {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "local_cookie_expiry_preserves_persistent_dates_across_clock_changes",
                "--nocapture",
            ])
            .env(CHILD, zone)
            .env("TZ", zone)
            .output()?;
        assert!(
            output.status.success(),
            "Timezone child {zone} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
