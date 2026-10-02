//! The backups beside the chosen vault: the list the window is shown, the one
//! it opens, and the one it makes the vault.
//!
//! A backup is named by the slot it was shown at, and a slot is an answer about
//! the chain as it stood: every save renames each backup a slot further down.
//! So what the window was shown is kept here, and a press opens the file that
//! was shown wherever the chain has moved it since - or nothing, and never its
//! neighbour.

use std::path::PathBuf;

use serde::Serialize;
use vault_core::Adopted;
use vault_core::storage::snapshot;

use super::Session;
use crate::error::Failure;

/// Why the chosen backup is chosen, for the strip over it to say.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Looking {
    /// The vault's own file would not open, or was not there, and this is
    /// what the unlock screen offered instead.
    Unopened,
    /// The reader asked to look at it, with the vault's file as it was.
    Asked,
}

/// The backups [`Session::snapshots`] last answered with.
pub(super) struct Listing {
    /// The file they are beside.
    beside: PathBuf,
    taken: Vec<snapshot::Taken>,
}

impl Listing {
    /// The backup that was shown at `index`.
    fn shown(&self, index: u32) -> Option<Shown> {
        let taken = self.taken.iter().find(|each| each.index == index)?;
        Some(Shown {
            beside: self.beside.clone(),
            taken: taken.clone(),
        })
    }
}

/// One backup the window was shown, to be found again.
pub(super) struct Shown {
    beside: PathBuf,
    taken: snapshot::Taken,
}

impl Shown {
    /// Where the backup is now, or nothing when it has been pushed out of the
    /// chain or taken away, or the chain cannot be read.
    pub(super) fn now(&self) -> Option<PathBuf> {
        snapshot::find(&self.beside, &self.taken)
            .ok()
            .flatten()
            .map(|found| found.path)
    }
}

impl Session {
    /// The backups of the chosen file - or, when the chosen file is itself a
    /// backup, of the vault it was taken beside - newest first. What a later
    /// [`Session::look`] means by a slot is this list.
    pub fn snapshots(&self) -> Result<Vec<snapshot::Taken>, Failure> {
        let mut held = self.held();
        let chosen = held.database.clone().ok_or_else(Failure::no_vault)?;
        let beside = snapshot::taken_from(&chosen).unwrap_or(chosen);
        let taken = snapshot::taken(&beside).map_err(Failure::io)?;
        held.listed = Some(Listing {
            beside,
            taken: taken.clone(),
        });
        Ok(taken)
    }

    /// Points the session at the backup the last list showed at `index`: now,
    /// when nothing is open, and after the lock when a vault is. Answers
    /// whether that lock has to follow.
    ///
    /// The vault open now is closed the ordinary way, so that everything it
    /// holds is written where it belongs first, and only a lock that kept all
    /// of it there points the session at the backup (see [`Session::lock`]).
    /// That lock's own save moves every backup a slot on, which is why the
    /// backup is found by the file it is. One that is no longer in the chain,
    /// or was never listed, is `gone`, and nothing changes: asked before the
    /// lock, so that a vault is never closed for a backup that is not there.
    /// Only that lock's own save can still push the oldest out on the way,
    /// and the vault's screen then says so (see [`Session::backup_gone`]).
    ///
    /// The key file is kept: a backup opens with the vault's credentials as
    /// they were when it was taken.
    pub fn look(&self, index: u32) -> Result<bool, Failure> {
        let mut held = self.held();
        let shown = held
            .listed
            .as_ref()
            .and_then(|listed| listed.shown(index))
            .ok_or_else(Failure::gone)?;

        let backup = shown.now().ok_or_else(Failure::gone)?;
        if held.open.get().is_some() {
            held.returning = false;
            held.looking_after = Some(shown);
            return Ok(true);
        }

        // A backup that would not open either passes on why the reader was
        // looking at backups at all.
        let because = match (held.unopened, held.looking) {
            (false, _) => Looking::Asked,
            (true, Some(was)) => was,
            (true, None) => Looking::Unopened,
        };
        held.point_beside(backup);
        held.looking = Some(because);
        Ok(false)
    }

    /// Why the chosen backup is chosen, while one is.
    pub fn looking(&self) -> Option<Looking> {
        self.held().looking
    }

    /// Makes the open backup the vault it was taken beside, and leaves the
    /// session open on that vault: see [`vault_core::Vault::adopt`]. Answers
    /// with the vault and what became of its file.
    ///
    /// Held to how the window was last told the vault's file stood, and
    /// written down for the next launch like any vault that opened. The key
    /// file stays: it opened the backup, and it opens the vault now.
    ///
    /// What became of the vault's file is kept with the vault as well, for a
    /// lock that takes the window before it has said so (see
    /// [`Session::adopted`]). A backup asked for, or a way back to the vault,
    /// pressed while this ran was about the backup, which is the vault now:
    /// both are let go, and the lock that follows them stays on the vault.
    pub fn adopt(&self) -> Result<(PathBuf, Adopted), Failure> {
        let (vault, adopted) = {
            let mut held = self.held();
            let shown = held.shown;
            let open = held.changing()?;
            let adopted = open.vault.adopt(shown);
            // Whatever the answer: a write that failed after the snapshots
            // moved on has moved the open backup with them.
            let path = open.vault.path().to_path_buf();
            if let Ok(made) = &adopted {
                open.adopted = Some((open.vault.edits(), made.clone()));
            }
            held.database = Some(path.clone());
            let adopted = adopted?;
            held.shown = None;
            held.looking = None;
            held.looking_after = None;
            held.returning = false;
            (path, adopted)
        };

        self.write_down(&vault);
        Ok((vault, adopted))
    }
}
