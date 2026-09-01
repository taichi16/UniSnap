const MAX_FONT_FAMILY_LENGTH: usize = 256;

fn normalize_font_names<I>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut fonts = names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| {
            !name.is_empty()
                && name.chars().count() <= MAX_FONT_FAMILY_LENGTH
                && !name.chars().any(char::is_control)
        })
        .collect::<Vec<_>>();

    fonts.sort_by_key(|name| name.to_lowercase());
    fonts.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    fonts
}

#[cfg(target_os = "windows")]
fn enumerate_system_fonts() -> Result<Vec<String>, String> {
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteLocalizedStrings, DWRITE_FACTORY_TYPE_SHARED,
    };

    fn first_localized_name(strings: &IDWriteLocalizedStrings) -> Result<Option<String>, String> {
        let count = unsafe { strings.GetCount() };
        if count == 0 {
            return Ok(None);
        }

        let length = unsafe { strings.GetStringLength(0) }
            .map_err(|error| format!("讀取系統字型名稱長度失敗：{error}"))?;
        let mut buffer = vec![0_u16; length as usize + 1];
        unsafe { strings.GetString(0, &mut buffer) }
            .map_err(|error| format!("讀取系統字型名稱失敗：{error}"))?;
        Ok(Some(String::from_utf16_lossy(&buffer[..length as usize])))
    }

    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|error| format!("建立 DirectWrite 字型服務失敗：{error}"))?;
    let mut collection = None;
    unsafe { factory.GetSystemFontCollection(&mut collection, false) }
        .map_err(|error| format!("取得 Windows 系統字型失敗：{error}"))?;
    let collection = collection.ok_or("Windows 未回傳系統字型集合")?;

    let family_count = unsafe { collection.GetFontFamilyCount() };
    let mut names = Vec::with_capacity(family_count as usize);
    for index in 0..family_count {
        let family = unsafe { collection.GetFontFamily(index) }
            .map_err(|error| format!("讀取第 {} 個字型家族失敗：{error}", index + 1))?;
        let localized = unsafe { family.GetFamilyNames() }
            .map_err(|error| format!("讀取第 {} 個字型名稱失敗：{error}", index + 1))?;
        if let Some(name) = first_localized_name(&localized)? {
            names.push(name);
        }
    }

    Ok(normalize_font_names(names))
}

#[tauri::command]
pub fn list_system_fonts() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        enumerate_system_fonts()
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(normalize_font_names([
            "Arial".to_string(),
            "Courier New".to_string(),
            "Georgia".to_string(),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_font_names;

    #[cfg(target_os = "windows")]
    #[test]
    fn enumerates_installed_windows_font_families() {
        let fonts = super::enumerate_system_fonts().expect("DirectWrite should enumerate fonts");
        assert!(!fonts.is_empty());
        assert!(fonts
            .windows(2)
            .all(|pair| pair[0].to_lowercase() <= pair[1].to_lowercase()));
    }

    #[test]
    fn normalizes_sorts_and_deduplicates_font_names() {
        let fonts = normalize_font_names([
            "  Georgia ".to_string(),
            "arial".to_string(),
            "Arial".to_string(),
            "".to_string(),
            "Bad\nFont".to_string(),
        ]);

        assert_eq!(fonts, vec!["arial", "Georgia"]);
    }

    #[test]
    fn rejects_unreasonably_long_font_names() {
        let fonts = normalize_font_names(["A".repeat(257), "Segoe UI".to_string()]);
        assert_eq!(fonts, vec!["Segoe UI"]);
    }
}
