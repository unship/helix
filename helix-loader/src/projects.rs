use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectsFile {
    projects: Vec<Project>,
}

pub fn projects_file_path() -> PathBuf {
    crate::config_dir().join("projects.toml")
}

pub fn load_projects() -> Result<Vec<Project>> {
    let file_path = projects_file_path();
    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read projects file: {}", file_path.display()))?;

    let projects_file: ProjectsFile = toml::from_str(&content)
        .with_context(|| format!("Failed to parse projects file: {}", file_path.display()))?;

    Ok(projects_file.projects)
}

pub fn save_projects(projects: &[Project]) -> Result<()> {
    let file_path = projects_file_path();
    crate::ensure_parent_dir(&file_path);

    let projects_file = ProjectsFile {
        projects: projects.to_vec(),
    };

    let content = toml::to_string_pretty(&projects_file)
        .context("Failed to serialize projects to TOML")?;

    std::fs::write(&file_path, content)
        .with_context(|| format!("Failed to write projects file: {}", file_path.display()))?;

    Ok(())
}

pub fn scan_git_repositories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut repositories = HashSet::new();
    let root = root.canonicalize()
        .with_context(|| format!("Failed to canonicalize root path: {}", root.display()))?;
    
    scan_directory(&root, &root, &mut repositories)?;
    
    let mut repos: Vec<PathBuf> = repositories.into_iter().collect();
    repos.sort();
    Ok(repos)
}

fn scan_directory(
    current: &Path,
    root: &Path,
    repositories: &mut HashSet<PathBuf>,
) -> Result<()> {
    // Canonicalize current path once
    let current_canonical = current.canonicalize()
        .unwrap_or_else(|_| current.to_path_buf());
    
    // Check if current directory is a .git directory
    if current.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == ".git")
        .unwrap_or(false)
        && current.is_dir()
    {
        // Get the parent directory (the repository root)
        if let Some(repo_root) = current.parent() {
            let repo_root = repo_root.canonicalize()
                .unwrap_or_else(|_| repo_root.to_path_buf());
            repositories.insert(repo_root);
            return Ok(()); // Don't scan inside .git directory
        }
    }
    
    // Check if current directory is inside any discovered repository
    for repo_root in repositories.iter() {
        if current_canonical.starts_with(repo_root) && current_canonical != *repo_root {
            // This path is inside a repository, skip it
            return Ok(());
        }
    }
    
    // Read directory entries
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(e) => {
            // Skip directories we can't read (permission denied, etc.)
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                return Ok(());
            }
            return Err(e).with_context(|| format!("Failed to read directory: {}", current.display()));
        }
    };
    
    for entry in entries {
        let entry = entry.with_context(|| format!("Failed to read entry in: {}", current.display()))?;
        let path = entry.path();
        
        // Skip if it's a symlink to avoid cycles (optional, but safer)
        if path.is_symlink() {
            continue;
        }
        
        if path.is_dir() {
            scan_directory(&path, root, repositories)?;
        }
    }
    
    Ok(())
}

pub fn update_project_last_accessed(projects: &mut [Project], path: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    for project in projects.iter_mut() {
        let project_path = project.path.canonicalize()
            .unwrap_or_else(|_| project.path.clone());
        if project_path == path {
            project.last_accessed = Some(now);
            break;
        }
    }
}
