//! Console flow: preview and confirm local sealing, without an SMB password.

use super::{Strings, confirm, input, select};
use crate::config::Config;
use crate::error::{BackupError, Result};
use crate::service::{Exit, seal};
use std::path::Path;

pub(crate) async fn run(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let Some(action) = select(s.seal_action_prompt, &s.seal_options, 0).await? else {
        return Ok(Exit::Success);
    };
    match action {
        1 => return backup_only(s, config_path).await,
        2 => return configure(s, config_path).await,
        3 => return set_passphrase(s, config_path).await,
        _ => {}
    }
    let config = Config::load(config_path)?;
    seal::run(&config, true).await?;
    if confirm(s.seal_run_prompt, s.yes, s.no, false).await? == Some(true) {
        seal::run(&config, false).await
    } else {
        Ok(Exit::Success)
    }
}

pub(crate) async fn backup_only(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;
    let request = seal::backup_request(&config, true)?;
    crate::service::backup::run(&config, request).await?;
    if confirm(s.sealed_backup_prompt, s.yes, s.no, false).await? != Some(true)
        || !super::ensure_password(s, &config).await?
    {
        return Ok(Exit::Success);
    }
    crate::service::backup::run(&config, seal::backup_request(&config, false)?).await
}

pub(crate) async fn configure(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    let config = Config::load(config_path)?;
    let default_path = config
        .sealed
        .as_ref()
        .and_then(|sealed| sealed.items.first())
        .map_or("~/.ssh", |item| item.path.as_str());
    let Some(path) = input(s.seal_source_prompt, Some(default_path)).await? else {
        return Ok(Exit::Success);
    };
    let default_archive = config
        .sealed
        .as_ref()
        .and_then(|sealed| sealed.items.iter().find(|item| item.path == path))
        .map_or("ssh.tar.gz.age", |item| item.archive.as_str());
    let Some(archive) = input(s.seal_archive_prompt, Some(default_archive)).await? else {
        return Ok(Exit::Success);
    };
    let staging = config
        .sealed
        .as_ref()
        .map_or("~/.local/share/heelonbackup/sealed", |sealed| {
            sealed.staging_dir.as_str()
        });
    let Some(staging) = input(s.seal_staging_prompt, Some(staging)).await? else {
        return Ok(Exit::Success);
    };
    let updated = seal::configure_item(&config, path, archive, staging)?;
    seal::run(&updated, true).await?;
    if confirm(s.seal_save_prompt, s.yes, s.no, false).await? == Some(true) {
        updated.save(config_path)?;
        crate::ui::say!(
            "Sealed configuration saved; existing items and ordinary sources preserved. Sensitive paths are automatically protected."
        );
    }
    Ok(Exit::Success)
}

pub(crate) async fn set_passphrase(s: &'static Strings, config_path: &Path) -> Result<Exit> {
    use age::secrecy::{ExposeSecret, SecretString};
    let config = Config::load(config_path)?;
    seal::run(&config, true).await?;
    if confirm(s.seal_secret_warning, s.yes, s.no, false).await? != Some(true) {
        return Ok(Exit::Success);
    }
    let Some(first) = super::prompt::password(s.seal_password_prompt).await? else {
        return Ok(Exit::Success);
    };
    let first = SecretString::from(first);
    let Some(second) = super::prompt::password(s.seal_password_confirm).await? else {
        return Ok(Exit::Success);
    };
    let second = SecretString::from(second);
    if first.expose_secret() != second.expose_secret() {
        return Err(BackupError::Seal(
            "passphrases do not match; keyring unchanged".into(),
        ));
    }
    seal::store_passphrase(&config, first).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SealSecret, SealedConfig, SealedItem};
    use crate::console::prompt::{self, Answer, Kind, Request};
    use crate::console::strings::Lang;
    use tokio::sync::mpsc;

    async fn answer(rx: &mut mpsc::UnboundedReceiver<Request>, text: Answer, masked: bool) {
        let request = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let Request::Ask { kind, reply, .. } = request else {
            panic!("expected dialog")
        };
        if masked {
            assert!(matches!(
                kind,
                Kind::Input {
                    masked: true,
                    default: None
                }
            ));
        }
        if let Kind::Select { default, .. } = kind {
            assert_eq!(default, 1, "destructive confirmations default to No");
        }
        reply.send(Some(text)).unwrap();
    }

    #[tokio::test]
    async fn configuration_confirmation_and_masked_mismatch_are_safe_in_both_languages() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("synthetic-keys");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("id_test"), b"synthetic").unwrap();
        let staging = dir.path().join("sealed");
        let path = dir.path().join("config.json");
        let config = Config {
            sealed: Some(SealedConfig {
                staging_dir: staging.to_string_lossy().into_owned(),
                secret: SealSecret::default(),
                items: vec![SealedItem {
                    path: source.to_string_lossy().into_owned(),
                    archive: "ssh.tar.gz.age".into(),
                }],
            }),
            ..Config::default()
        };
        config.save(&path).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        prompt::connect(Some(tx));
        for lang in [Lang::En, Lang::Fr] {
            let s = lang.strings();
            let original = std::fs::read(&path).unwrap();
            let operation = tokio::spawn({
                let path = path.clone();
                async move { configure(s, &path).await }
            });
            answer(
                &mut rx,
                Answer::Text(source.to_string_lossy().into_owned()),
                false,
            )
            .await;
            answer(&mut rx, Answer::Text("renamed.tar.gz.age".into()), false).await;
            answer(
                &mut rx,
                Answer::Text(staging.to_string_lossy().into_owned()),
                false,
            )
            .await;
            answer(&mut rx, Answer::Index(1), false).await;
            assert_eq!(operation.await.unwrap().unwrap(), Exit::Success);
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert!(!staging.exists());

            let operation = tokio::spawn({
                let path = path.clone();
                async move { set_passphrase(s, &path).await }
            });
            answer(&mut rx, Answer::Index(0), false).await;
            answer(&mut rx, Answer::Text("synthetic-first".into()), true).await;
            answer(&mut rx, Answer::Text("synthetic-second".into()), true).await;
            let error = operation.await.unwrap().unwrap_err().to_string();
            assert!(error.contains("do not match"));
            assert!(!error.contains("synthetic"));
        }
        let s = Lang::En.strings();
        let operation = tokio::spawn({
            let path = path.clone();
            async move { configure(s, &path).await }
        });
        answer(
            &mut rx,
            Answer::Text(source.to_string_lossy().into_owned()),
            false,
        )
        .await;
        answer(&mut rx, Answer::Text("renamed.tar.gz.age".into()), false).await;
        answer(
            &mut rx,
            Answer::Text(staging.to_string_lossy().into_owned()),
            false,
        )
        .await;
        answer(&mut rx, Answer::Index(0), false).await;
        assert_eq!(operation.await.unwrap().unwrap(), Exit::Success);
        assert_eq!(
            Config::load(&path).unwrap().sealed.unwrap().items[0].archive,
            "renamed.tar.gz.age"
        );
        assert!(!staging.exists());
        let history = dir.path().join("history");
        std::fs::create_dir(&history).unwrap();
        let summary = history.join("20261106_120000.json");
        std::fs::write(&summary, b"synthetic summary").unwrap();
        std::fs::write(history.join("config.json"), b"keep").unwrap();
        for choice in [1, 0] {
            let cleanup = crate::service::history::Cleanup::at(history.clone()).unwrap();
            let operation =
                tokio::spawn(
                    async move { crate::console::status::clear_history(s, cleanup).await },
                );
            answer(&mut rx, Answer::Index(choice), false).await;
            assert_eq!(operation.await.unwrap().unwrap(), Exit::Success);
            assert_eq!(summary.exists(), choice == 1);
            assert_eq!(std::fs::read(history.join("config.json")).unwrap(), b"keep");
        }
        prompt::connect(None);
    }
}
