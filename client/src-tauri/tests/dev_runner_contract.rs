#[test]
fn macos_dev_runner_launches_signed_app_bundle_for_tcc_privacy_prompts() {
    let info_plist = include_str!("../Info.plist");
    let dev_runner = include_str!("../scripts/tauri-dev-runner.sh");

    assert!(
        info_plist.contains("NSMicrophoneUsageDescription")
            && info_plist.contains("NSSpeechRecognitionUsageDescription"),
        "Info.plist must keep microphone and speech-recognition privacy strings for native dictation"
    );
    assert!(
        dev_runner.contains("Cadence Dev.app")
            && dev_runner.contains("contents_dir=\"$app_bundle/Contents\"")
            && dev_runner.contains("macos_dir=\"$contents_dir/MacOS\"")
            && dev_runner.contains("info_plist=\"$contents_dir/Info.plist\"")
            && dev_runner.contains("sync_resources_if_present")
            && dev_runner.contains("exec \"$bundled_executable\""),
        "tauri dev runner must launch from a signed .app wrapper so macOS TCC can read dictation privacy strings"
    );
    assert!(
        dev_runner.contains("CFBundleExecutable")
            && dev_runner.contains("CFBundleIdentifier")
            && dev_runner.contains("info_template"),
        "tauri dev runner must preserve a complete app-bundle Info.plist for native speech authorization"
    );
}

#[test]
fn modern_dictation_does_not_request_legacy_speech_authorization() {
    let modern_engine = include_str!("../native/dictation/ModernAvailable.swift");
    let legacy_engine = include_str!("../native/dictation/LegacyEngine.swift");

    assert!(
        !modern_engine.contains("SFSpeechRecognizer.requestAuthorization"),
        "modern on-device dictation must not invoke the legacy SFSpeech authorization prompt"
    );
    assert!(
        !modern_engine.contains("dictation_speech_permission_denied"),
        "modern on-device dictation should not block startup on legacy speech-recognition permission"
    );
    assert!(
        legacy_engine.contains("SFSpeechRecognizer.requestAuthorization"),
        "legacy dictation still owns the SFSpeech authorization prompt"
    );
}
