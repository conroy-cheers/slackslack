use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BrowserProfile {
    pub label: String,
    pub local_storage: PathBuf,
    pub cookies: PathBuf,
}

pub fn browser_profiles() -> Vec<BrowserProfile> {
    let home = match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home),
        Err(_) => return Vec::new(),
    };

    let mut profiles = Vec::new();
    let roots = [
        ("Chrome", home.join(".config").join("google-chrome")),
        ("Chromium", home.join(".config").join("chromium")),
        ("Brave", home.join(".config").join("BraveSoftware").join("Brave-Browser")),
        ("Vivaldi", home.join(".config").join("vivaldi")),
        ("Edge", home.join(".config").join("microsoft-edge")),
    ];

    for (browser_name, root) in roots {
        profiles.extend(browser_profiles_for_root(browser_name, root));
    }

    profiles
}

fn browser_profiles_for_root(browser_name: &str, root: PathBuf) -> Vec<BrowserProfile> {
    if !root.exists() {
        return Vec::new();
    }

    let mut profile_dirs = Vec::new();
    for name in ["Default", "Profile 1", "Profile 2", "Profile 3", "Profile 4", "Profile 5"] {
        let dir = root.join(name);
        if dir.exists() {
            profile_dirs.push(dir);
        }
    }

    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name == "Default" || name.starts_with("Profile ") {
                if !profile_dirs.iter().any(|existing| existing == &path) {
                    profile_dirs.push(path);
                }
            }
        }
    }

    profile_dirs
        .into_iter()
        .map(|profile_dir| {
            let profile_name = profile_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown");
            let cookies = {
                let network = profile_dir.join("Network").join("Cookies");
                if network.exists() {
                    network
                } else {
                    profile_dir.join("Cookies")
                }
            };
            BrowserProfile {
                label: format!("{browser_name} {profile_name}"),
                local_storage: profile_dir.join("Local Storage").join("leveldb"),
                cookies,
            }
        })
        .collect()
}
