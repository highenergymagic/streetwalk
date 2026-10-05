#[test]
#[ignore = "Requires the downloaded x64 controller DLL and a running NVDA instance"]
fn controller_connects_to_nvda() {
    let path = std::env::current_dir()
        .unwrap()
        .join("dist/nvda-controller/x64/nvdaControllerClient.dll");
    unsafe {
        let library = libloading::Library::new(path).unwrap();
        let test = library
            .get::<unsafe extern "system" fn() -> i32>(b"nvdaController_testIfRunning\0")
            .unwrap();
        library
            .get::<unsafe extern "system" fn(*const u16) -> i32>(b"nvdaController_speakText\0")
            .unwrap();
        library
            .get::<unsafe extern "system" fn() -> i32>(b"nvdaController_cancelSpeech\0")
            .unwrap();
        let status = test();
        println!("NVDA controller connection result: {status}");
        assert_eq!(status, 0, "NVDA must be running for this opt-in check");
    }
}
