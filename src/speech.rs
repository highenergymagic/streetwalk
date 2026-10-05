use libloading::Library;
use windows_sys::Win32::System::StationsAndDesktops::*;
pub struct Speech {
    library: Option<Library>,
    pub status: String,
}
impl Speech {
    pub fn new() -> Self {
        let dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_owned();
        let mut library = None;
        for name in ["nvdaControllerClient.dll", "nvdaControllerClient64.dll"] {
            if let Ok(lib) = unsafe { Library::new(dir.join(name)) } {
                library = Some(lib);
                break;
            }
        }
        let status = if let Some(lib) = &library {
            unsafe {
                match lib
                    .get::<unsafe extern "system" fn() -> i32>(b"nvdaController_testIfRunning\0")
                {
                    Ok(test) if test() == 0 => "NVDA connected",
                    _ => "NVDA is not running",
                }
            }
        } else {
            "NVDA controller DLL missing; announcements are still displayed"
        }
        .into();
        Self { library, status }
    }
    pub fn stop(&self) {
        if let Some(lib) = &self.library {
            unsafe {
                if let Ok(cancel) =
                    lib.get::<unsafe extern "system" fn() -> i32>(b"nvdaController_cancelSpeech\0")
                {
                    cancel();
                }
            }
        }
    }
    pub fn say(&self, text: &str) {
        unsafe {
            // Do not speak into a locked or secure desktop.
            let desktop = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
            if desktop.is_null() {
                return;
            }
            let mut name = [0u16; 128];
            let mut needed = 0;
            let ok = GetUserObjectInformationW(
                desktop,
                UOI_NAME,
                name.as_mut_ptr().cast(),
                256,
                &mut needed,
            );
            CloseDesktop(desktop);
            if ok == 0
                || String::from_utf16_lossy(
                    &name[..name.iter().position(|c| *c == 0).unwrap_or(128)],
                ) != "Default"
            {
                return;
            }
            if let Some(lib) = &self.library {
                if let Ok(speak) = lib.get::<unsafe extern "system" fn(*const u16) -> i32>(
                    b"nvdaController_speakText\0",
                ) {
                    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
                    speak(text.as_ptr());
                }
            }
        }
    }
}
