use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

pub struct TestWorkspace {
    pub dir: TempDir,
}

impl TestWorkspace {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temporary test directory");
        let ws = Self { dir };

        ws.git(&["init"]);
        ws.git(&["config", "user.name", "Diffmem Test"]);
        ws.git(&["config", "user.email", "diffmem@test.local"]);
        ws.git(&["config", "core.autocrlf", "false"]);

        ws
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) -> Output {
        let output = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .expect("failed to run git command");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("git command failed: {:?}\nStderr: {}", args, stderr);
        }
        output
    }

    pub fn write_file(&self, rel_path: &str, content: &str) -> PathBuf {
        let full_path = self.path().join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent directories");
        }
        fs::write(&full_path, content).expect("failed to write file");
        full_path
    }

    pub fn read_file(&self, rel_path: &str) -> String {
        let full_path = self.path().join(rel_path);
        fs::read_to_string(&full_path).unwrap_or_else(|e| {
            panic!("failed to read file {}: {}", full_path.display(), e)
        })
    }

    pub fn file_exists(&self, rel_path: &str) -> bool {
        self.path().join(rel_path).exists()
    }
}
