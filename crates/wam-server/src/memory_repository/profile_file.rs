//! Private profile snapshots for the single-process local repository.

use super::{Account, State};
use crate::{
    ports::RepositoryError,
    profile_wire::{profile_response, validate_profile},
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    collections::HashSet,
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use wam_contracts::ProfilePutRequest;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredAccount {
    id: String,
    profile: ProfilePutRequest,
    seed_points: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u8,
    accounts: Vec<StoredAccount>,
}

fn unavailable(_: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Unavailable
}

pub(super) fn load_or_initialize(path: &Path, seed: &State) -> Result<State, RepositoryError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(parent).map_err(unavailable)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            #[cfg(unix)]
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(unavailable)?;
            let snapshot: Snapshot = serde_json::from_slice(&fs::read(path).map_err(unavailable)?)
                .map_err(unavailable)?;
            if snapshot.schema_version != 1 {
                return Err(RepositoryError::Unavailable);
            }
            let mut ids = HashSet::new();
            let mut handles = HashSet::new();
            let mut accounts = Vec::new();
            for account in snapshot.accounts {
                if account.id.is_empty()
                    || account.id.len() > 256
                    || account.seed_points < 0
                    || !ids.insert(account.id.clone())
                {
                    return Err(RepositoryError::Unavailable);
                }
                let (profile, public_opt_in) =
                    validate_profile(account.profile).map_err(unavailable)?;
                if !handles.insert(profile.handle().as_str().to_owned()) {
                    return Err(RepositoryError::Unavailable);
                }
                accounts.push(Account {
                    id: account.id,
                    profile,
                    public_opt_in,
                    points: account.seed_points,
                });
            }
            Ok(State { accounts })
        }
        Ok(_) => Err(RepositoryError::Unavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            persist(path, seed)?;
            Ok(seed.clone())
        }
        Err(error) => Err(unavailable(error)),
    }
}

pub(super) fn persist(path: &Path, state: &State) -> Result<(), RepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(RepositoryError::Unavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(unavailable(error)),
    }
    let snapshot = Snapshot {
        schema_version: 1,
        accounts: state
            .accounts
            .iter()
            .map(|account| {
                let wire = profile_response(&account.profile, account.public_opt_in);
                StoredAccount {
                    id: account.id.clone(),
                    profile: ProfilePutRequest {
                        handle: wire.handle,
                        bio: wire.bio,
                        service_links: wire.service_links,
                        public_opt_in: wire.public_opt_in,
                    },
                    seed_points: account.points,
                }
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&snapshot).map_err(unavailable)?;
    let temporary = temporary_path(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary).map_err(unavailable)?;
    let result = (|| {
        file.write_all(&bytes).map_err(unavailable)?;
        file.sync_all().map_err(unavailable)?;
        fs::rename(&temporary, path).map_err(unavailable)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(unavailable)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn temporary_path(path: &Path) -> Result<PathBuf, RepositoryError> {
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).map_err(unavailable)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RepositoryError::Unavailable)?;
    Ok(path.with_file_name(format!(
        ".{filename}.{}.{:016x}.tmp",
        std::process::id(),
        u64::from_le_bytes(random)
    )))
}
