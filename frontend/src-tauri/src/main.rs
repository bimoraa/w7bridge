fn main() -> Result<(), tauri::Error> {

    tauri::Builder::default().run(tauri::generate_context!())

}
