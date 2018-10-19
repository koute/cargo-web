use directories::ProjectDirs;

lazy_static! {
    pub static ref PROJECT_DIRS: ProjectDirs = ProjectDirs::from( "", "Jan Bujak",  "cargo-web" ).unwrap();
}
