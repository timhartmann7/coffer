//! Temporary probes.

use keepass::db::fields;
use vault_core::model::Project;

use crate::support::{BUILT_PASSWORD, built, open};

fn dump(project: &Project, depth: usize) {
    eprintln!(
        "{}name={:?} bin={} entries={}",
        "  ".repeat(depth),
        project.name,
        project.is_recycle_bin,
        project.entries.len()
    );
    for s in &project.sections {
        dump(s, depth + 1);
    }
}

#[test]
fn probe_root_named_as_the_bin() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "rootbin.kdbx", |database| {
        let root = database.root().id();
        database.meta.recyclebin_enabled = Some(true);
        database.meta.recyclebin_uuid = Some(root.uuid());
        let a = database.root_mut().add_group().id();
        {
            let mut g = database.group_mut(a).expect("a");
            g.name = "work".to_owned();
            g.add_group().name = "nested".to_owned();
            g.add_entry().edit(|e| {
                e.set_unprotected(fields::TITLE, "in work");
            });
        }
        database.root_mut().add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "top");
        });
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    eprintln!("entries before {}", vault.count());
    dump(&vault.tree(), 0);
    let outcome = vault.empty_recycle_bin();
    eprintln!(
        "empty -> {:?}",
        outcome.as_ref().err().map(|e| format!("{e:?}"))
    );
    eprintln!("entries after {}", vault.count());
    dump(&vault.tree(), 0);
}

#[test]
fn probe_new_entry_and_group_times() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "times.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let id = vault.create_entry(root).expect("entry");
    let e = vault.entry(id).expect("there");
    eprintln!("entry times {:?}", e.times);
}
