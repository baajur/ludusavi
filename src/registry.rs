use crate::prelude::Error;
use winreg::types::{FromRegValue, ToRegValue};

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Hives(pub std::collections::HashMap<String, Keys>);

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Keys(pub std::collections::HashMap<String, Entries>);

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Entries(pub std::collections::HashMap<String, Entry>);

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    sz: Option<String>,
    #[serde(rename = "expandSz")]
    expand_sz: Option<String>,
    #[serde(rename = "multiSz")]
    multi_sz: Option<String>,
    dword: Option<u32>,
    qword: Option<u64>,
}

pub struct RegistryInfo {
    pub found: bool,
}

impl Hives {
    pub fn load(file: &std::path::PathBuf) -> Option<Self> {
        if crate::path::is_file(&crate::path::render_pathbuf(&file)) {
            let content = std::fs::read_to_string(&file).ok()?;
            serde_yaml::from_str(&content).ok()
        } else {
            None
        }
    }

    pub fn save(&self, file: &std::path::PathBuf) {
        let mut folder = file.clone();
        folder.pop();
        if std::fs::create_dir_all(folder).is_ok() {
            std::fs::write(file, serde_yaml::to_string(self).unwrap().as_bytes()).unwrap();
        }
    }

    pub fn store_key_from_full_path(&mut self, path: &str) -> Result<RegistryInfo, Error> {
        let path = path.replace('/', "\\");

        let parts: Vec<&str> = path.splitn(2, '\\').collect();
        if parts.len() != 2 {
            return Err(Error::RegistryIssue);
        }

        let hive_name = parts[0];
        let hive = get_hkey_from_name(hive_name).ok_or(Error::RegistryIssue)?;
        let key = parts[1];

        let info = self.store_key(hive, hive_name, key)?;

        Ok(info)
    }

    pub fn store_key(&mut self, hive: winreg::HKEY, hive_name: &str, key: &str) -> Result<RegistryInfo, Error> {
        let subkey = winreg::RegKey::predef(hive)
            .open_subkey(key)
            .map_err(|_| Error::RegistryIssue)?;

        let mut info = RegistryInfo { found: false };
        for (name, value) in subkey.enum_values().filter_map(|x| x.ok()) {
            let entry = Entry::from(value);
            if entry.is_set() {
                info.found = true;
                self.0
                    .entry(hive_name.to_string())
                    .or_insert_with(Default::default)
                    .0
                    .entry(key.to_string())
                    .or_insert_with(Default::default)
                    .0
                    .entry(name.to_string())
                    .or_insert_with(|| entry);
            }
        }

        let mut failed = false;
        for name in subkey.enum_keys().filter_map(|x| x.ok()) {
            if self.store_key(hive, hive_name, &format!("{}\\{}", key, name)).is_err() {
                failed = true;
            }
        }

        if failed {
            return Err(Error::RegistryIssue);
        }

        Ok(info)
    }

    pub fn restore(&self) -> Result<(), Error> {
        let mut failed = false;

        for (hive_name, keys) in self.0.iter() {
            let hive = match get_hkey_from_name(hive_name) {
                Some(x) => winreg::RegKey::predef(x),
                None => {
                    failed = true;
                    continue;
                }
            };

            for (key_name, entries) in keys.0.iter() {
                let (key, _) = match hive.create_subkey(key_name) {
                    Ok(x) => x,
                    Err(_) => {
                        failed = true;
                        continue;
                    }
                };

                for (entry_name, entry) in entries.0.iter() {
                    if let Some(value) = Option::<winreg::RegValue>::from(entry) {
                        if key.set_raw_value(entry_name, &value).is_err() {
                            failed = true;
                        }
                    } else {
                        failed = true;
                    }
                }
            }
        }

        if failed {
            return Err(Error::RegistryIssue);
        }

        Ok(())
    }
}

impl Entry {
    pub fn is_set(&self) -> bool {
        self.sz.is_some()
            || self.expand_sz.is_some()
            || self.multi_sz.is_some()
            || self.dword.is_some()
            || self.qword.is_some()
    }
}

impl From<winreg::RegValue> for Entry {
    fn from(item: winreg::RegValue) -> Self {
        match item.vtype {
            winreg::enums::RegType::REG_SZ => Self {
                sz: Some(String::from_reg_value(&item).unwrap_or_default()),
                ..Default::default()
            },
            winreg::enums::RegType::REG_EXPAND_SZ => Self {
                expand_sz: Some(String::from_reg_value(&item).unwrap_or_default()),
                ..Default::default()
            },
            winreg::enums::RegType::REG_MULTI_SZ => Self {
                multi_sz: Some(String::from_reg_value(&item).unwrap_or_default()),
                ..Default::default()
            },
            winreg::enums::RegType::REG_DWORD => Self {
                dword: Some(u32::from_reg_value(&item).unwrap_or_default()),
                ..Default::default()
            },
            winreg::enums::RegType::REG_QWORD => Self {
                qword: Some(u64::from_reg_value(&item).unwrap_or_default()),
                ..Default::default()
            },
            _ => Default::default(),
        }
    }
}

impl From<&Entry> for Option<winreg::RegValue> {
    fn from(item: &Entry) -> Option<winreg::RegValue> {
        if let Some(x) = &item.sz {
            Some(x.to_reg_value())
        } else if let Some(x) = &item.multi_sz {
            Some(x.to_reg_value())
        } else if let Some(x) = &item.expand_sz {
            Some(x.to_reg_value())
        } else if let Some(x) = &item.dword {
            Some(x.to_reg_value())
        } else if let Some(x) = &item.qword {
            Some(x.to_reg_value())
        } else {
            None
        }
    }
}

fn get_hkey_from_name(name: &str) -> Option<winreg::HKEY> {
    match name {
        "HKEY_CURRENT_USER" => Some(winreg::enums::HKEY_CURRENT_USER),
        "HKEY_LOCAL_MACHINE" => Some(winreg::enums::HKEY_LOCAL_MACHINE),
        _ => None,
    }
}

pub fn game_registry_backup_file(start: &str, game: &str) -> std::path::PathBuf {
    let mut path = crate::prelude::game_backup_dir(&start, &game);
    path.push("other");
    path.push("registry.yaml");
    path
}
