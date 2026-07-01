sed -i '/mod tests {/r /dev/stdin' crates/rustodian-scanner/src/scanner.rs << 'INNER'

    #[test]
    fn test_scanner_symlink_loop() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&b, a.join("link_to_b")).unwrap();
            std::os::unix::fs::symlink(&a, b.join("link_to_a")).unwrap();
        }

        File::create(a.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 5,
            follow_symlinks: true,
            exclude_patterns: vec![],
        };

        let projs = scanner.scan(root, &config).unwrap();
        assert!(!projs.is_empty());
    }

    #[test]
    fn test_scanner_no_read_permissions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("my_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();

        let unreadable = root.join("unreadable");
        fs::create_dir_all(&unreadable).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        }

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "my_proj");
    }

    #[test]
    fn test_scanner_malformed_manifest() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("multi_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();
        File::create(proj.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "multi_proj");
        assert_eq!(projs[0].languages.len(), 2);
    }
INNER
