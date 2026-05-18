use std::path::PathBuf;

use crate::{
    action::{EntryInfo, Side},
    components::{
        dialog::{MenuAction, MenuItem},
        panel::{is_archive, is_executable},
    },
};

// --- Context passed to every provider ---

/// Snapshot of panel state passed to each provider when building the F2 menu.
/// All fields are owned so providers can move values into `MenuAction` variants.
pub struct MenuCtx {
    pub entry: EntryInfo,
    /// Absolute path of the cursor entry (`panel_dir.join(&entry.name)`).
    pub entry_path: PathBuf,
    pub panel_dir: PathBuf,
    pub other_dir: PathBuf,
    pub active_side: Side,
    /// Marked files if any, otherwise just the cursor entry — same semantics as
    /// `Panel::effective_targets`.
    pub effective_targets: Vec<PathBuf>,
}

// --- Trait ---

/// Implement this to contribute items to the F2 context menu.
///
/// Each provider inspects `MenuCtx` and returns zero or more `MenuItem`s.
/// An empty `Vec` means the provider has nothing to offer for this entry.
pub trait ContextMenuProvider: Send + Sync {
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem>;
}

// --- Registry ---

/// Returns the built-in providers in their default display order.
pub(crate) fn builtin_providers() -> Vec<Box<dyn ContextMenuProvider>> {
    vec![
        Box::new(OsOpenProvider),
        Box::new(ArchiveProvider),
        Box::new(ExecuteProvider),
        Box::new(ChownProvider),
        Box::new(DeviceProvider),
    ]
}

// --- OsOpenProvider ---

/// Always-present: open with the OS default handler and launch VS Code.
pub struct OsOpenProvider;

impl ContextMenuProvider for OsOpenProvider {
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem> {
        let code_dir = if ctx.entry.is_dir {
            ctx.entry_path.clone()
        } else {
            ctx.panel_dir.clone()
        };
        vec![
            MenuItem::new("Open with OS (xdg-open)", MenuAction::OpenWithOs(ctx.entry_path.clone())),
            MenuItem::new("Run VS Code here", MenuAction::RunCodeHere(code_dir)),
        ]
    }
}

// --- ArchiveProvider ---

/// Extract archive files into the current or opposite panel directory.
pub struct ArchiveProvider;

impl ContextMenuProvider for ArchiveProvider {
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem> {
        let archives: Vec<PathBuf> = ctx
            .effective_targets
            .iter()
            .filter(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                is_archive(&name)
            })
            .cloned()
            .collect();

        if archives.is_empty() {
            return vec![];
        }

        let label = if archives.len() == 1 {
            archives[0]
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            format!("{} archives", archives.len())
        };

        vec![
            MenuItem::new(
                format!("Extract {label} here"),
                MenuAction::ExtractHere {
                    archives: archives.clone(),
                    dest: ctx.panel_dir.clone(),
                },
            ),
            MenuItem::new(
                format!("Extract {label} to → {}", ctx.other_dir.display()),
                MenuAction::ExtractHere {
                    archives,
                    dest: ctx.other_dir.clone(),
                },
            ),
        ]
    }
}

// --- ExecuteProvider ---

/// Run an executable with optional arguments.
pub struct ExecuteProvider;

impl ContextMenuProvider for ExecuteProvider {
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem> {
        if ctx.entry.is_dir || !is_executable(&ctx.entry_path) {
            return vec![];
        }
        vec![MenuItem::new("Execute…", MenuAction::RequestExecute(ctx.entry_path.clone()))]
    }
}

// --- ChownProvider ---

/// Change file ownership via `sudo chown`.
pub struct ChownProvider;

impl ContextMenuProvider for ChownProvider {
    fn items(&self, ctx: &MenuCtx) -> Vec<MenuItem> {
        vec![MenuItem::new(
            format!("Change owner… (now: {})", ctx.entry.owner),
            MenuAction::Chown {
                paths: ctx.effective_targets.clone(),
                current_owner: ctx.entry.owner.clone(),
                reload_sides: vec![ctx.active_side],
            },
        )]
    }
}

// --- DeviceProvider ---

/// Mount / unmount removable storage devices discovered via `lsblk`.
pub struct DeviceProvider;

impl ContextMenuProvider for DeviceProvider {
    fn items(&self, _ctx: &MenuCtx) -> Vec<MenuItem> {
        enumerate_removable_devices()
            .into_iter()
            .filter_map(|dev| {
                if let Some(mp) = &dev.mountpoint {
                    Some(MenuItem::new(
                        format!("Unmount {} — {} at {}", dev.name, dev.human_label(), mp),
                        MenuAction::UnmountDevice { device: dev.name.clone() },
                    ))
                } else if dev.fstype.is_some() {
                    Some(MenuItem::new(
                        format!("Mount {} — {}", dev.name, dev.human_label()),
                        MenuAction::MountDevice { device: dev.name.clone() },
                    ))
                } else {
                    None
                }
            })
            .collect()
    }
}

// --- Removable device enumeration via lsblk ---

pub struct RemovableDev {
    name: String,
    size: Option<String>,
    fstype: Option<String>,
    label: Option<String>,
    model: Option<String>,
    mountpoint: Option<String>,
}

impl RemovableDev {
    fn human_label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(s) = &self.size { parts.push(s); }
        if let Some(fs) = &self.fstype { parts.push(fs); }
        if let Some(lbl) = &self.label { parts.push(lbl); }
        if let Some(model) = &self.model { parts.push(model); }
        if parts.is_empty() { self.name.clone() } else { parts.join(", ") }
    }
}

fn enumerate_removable_devices() -> Vec<RemovableDev> {
    let Ok(out) = std::process::Command::new("lsblk")
        .args(["-J", "-o", "NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT,MODEL,HOTPLUG,TYPE"])
        .output()
    else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    if let Some(devs) = json["blockdevices"].as_array() {
        for dev in devs {
            collect_removable(dev, &mut result, dev["model"].as_str().map(str::trim));
        }
    }
    result
}

pub fn collect_removable<'a>(
    node: &'a serde_json::Value,
    out: &mut Vec<RemovableDev>,
    parent_model: Option<&'a str>,
) {
    let hotplug = node["hotplug"].as_bool().unwrap_or(false)
        || node["hotplug"].as_str().map(|s| s == "1").unwrap_or(false);
    let kind = node["type"].as_str().unwrap_or("");
    let model = node["model"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(parent_model)
        .map(str::to_owned);

    if hotplug && (kind == "part" || kind == "disk") {
        let fstype = node["fstype"].as_str().filter(|s| !s.is_empty()).map(String::from);
        if fstype.is_some() || kind == "part" {
            out.push(RemovableDev {
                name: node["name"].as_str().unwrap_or("").to_owned(),
                size: node["size"].as_str().filter(|s| !s.is_empty()).map(String::from),
                fstype,
                label: node["label"].as_str().filter(|s| !s.is_empty()).map(String::from),
                model,
                mountpoint: node["mountpoint"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            });
            return;
        }
    }

    if let Some(children) = node["children"].as_array() {
        for child in children {
            collect_removable(child, out, model.as_deref().or(parent_model));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{EntryInfo, Side};

    fn dummy_entry(name: &str, is_dir: bool) -> EntryInfo {
        EntryInfo {
            name: name.to_owned(),
            is_dir,
            is_symlink: false,
            size: 0,
            modified: 0,
            nlink: 1,
            owner: "user".to_owned(),
        }
    }

    fn ctx(name: &str, is_dir: bool) -> MenuCtx {
        let entry = dummy_entry(name, is_dir);
        let entry_path = PathBuf::from("/tmp").join(name);
        MenuCtx {
            entry,
            entry_path: entry_path.clone(),
            panel_dir: PathBuf::from("/tmp"),
            other_dir: PathBuf::from("/other"),
            active_side: Side::Left,
            effective_targets: vec![entry_path],
        }
    }

    #[test]
    fn os_open_provider_always_returns_two_items() {
        let items = OsOpenProvider.items(&ctx("file.txt", false));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn os_open_provider_on_dir_uses_entry_path_for_vscode() {
        let items = OsOpenProvider.items(&ctx("docs", true));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn archive_provider_returns_empty_for_non_archive() {
        let items = ArchiveProvider.items(&ctx("readme.txt", false));
        assert!(items.is_empty());
    }

    #[test]
    fn archive_provider_returns_two_items_for_zip() {
        let mut c = ctx("archive.zip", false);
        c.effective_targets = vec![PathBuf::from("/tmp/archive.zip")];
        let items = ArchiveProvider.items(&c);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn archive_provider_uses_count_label_for_multiple_archives() {
        let mut c = ctx("a.zip", false);
        c.effective_targets = vec![
            PathBuf::from("/tmp/a.zip"),
            PathBuf::from("/tmp/b.tar.gz"),
        ];
        let items = ArchiveProvider.items(&c);
        assert_eq!(items.len(), 2);
        assert!(items[0].label.contains("2 archives"));
    }

    #[test]
    fn archive_provider_returns_empty_for_directory() {
        let items = ArchiveProvider.items(&ctx("docs", true));
        assert!(items.is_empty());
    }

    #[test]
    fn execute_provider_returns_empty_for_directory() {
        let items = ExecuteProvider.items(&ctx("bin", true));
        assert!(items.is_empty());
    }

    #[test]
    fn chown_provider_always_returns_one_item() {
        let items = ChownProvider.items(&ctx("file.txt", false));
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn chown_provider_item_includes_current_owner() {
        let mut c = ctx("file.txt", false);
        c.entry.owner = "alice".to_owned();
        let items = ChownProvider.items(&c);
        assert!(items[0].label.contains("alice"));
    }

    #[test]
    fn removable_dev_human_label_all_fields() {
        let dev = RemovableDev {
            name: "sdb1".into(),
            size: Some("16G".into()),
            fstype: Some("vfat".into()),
            label: Some("USB".into()),
            model: Some("SanDisk".into()),
            mountpoint: None,
        };
        let label = dev.human_label();
        assert!(label.contains("16G"));
        assert!(label.contains("vfat"));
        assert!(label.contains("USB"));
        assert!(label.contains("SanDisk"));
    }

    #[test]
    fn removable_dev_human_label_empty_falls_back_to_name() {
        let dev = RemovableDev {
            name: "sdb".into(),
            size: None,
            fstype: None,
            label: None,
            model: None,
            mountpoint: None,
        };
        assert_eq!(dev.human_label(), "sdb");
    }

    #[test]
    fn collect_removable_hotplug_partition_is_included() {
        let node = serde_json::json!({
            "name": "sdb1",
            "size": "8G",
            "fstype": "vfat",
            "label": "USB",
            "mountpoint": "/mnt/usb",
            "model": "",
            "hotplug": true,
            "type": "part"
        });
        let mut out = Vec::new();
        collect_removable(&node, &mut out, None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "sdb1");
        assert_eq!(out[0].mountpoint.as_deref(), Some("/mnt/usb"));
    }

    #[test]
    fn collect_removable_non_hotplug_is_excluded() {
        let node = serde_json::json!({
            "name": "sda1",
            "size": "500G",
            "fstype": "ext4",
            "label": null,
            "mountpoint": "/",
            "model": "",
            "hotplug": false,
            "type": "part"
        });
        let mut out = Vec::new();
        collect_removable(&node, &mut out, None);
        assert!(out.is_empty());
    }

    #[test]
    fn collect_removable_recurses_into_children() {
        let node = serde_json::json!({
            "name": "sdb",
            "size": "16G",
            "fstype": null,
            "label": null,
            "mountpoint": null,
            "model": "SanDisk",
            "hotplug": true,
            "type": "disk",
            "children": [{
                "name": "sdb1",
                "size": "16G",
                "fstype": "vfat",
                "label": "DATA",
                "mountpoint": null,
                "model": null,
                "hotplug": true,
                "type": "part"
            }]
        });
        let mut out = Vec::new();
        collect_removable(&node, &mut out, None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "sdb1");
        assert_eq!(out[0].model.as_deref(), Some("SanDisk"));
    }
}
