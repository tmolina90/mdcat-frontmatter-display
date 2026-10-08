// Copyright 2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Test the command line interface of mdcat

#![deny(warnings, clippy::all)]

mod cli {
    use std::ffi::OsStr;
    use std::io::{Read, Write};
    use std::process::{Command, Output, Stdio};

    fn cargo_mdcat() -> Command {
        Command::new(env!("CARGO_BIN_EXE_mdcat"))
    }

    fn run_cargo_mdcat<I, S>(args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        cargo_mdcat().args(args).output().unwrap()
    }

    #[test]
    fn show_help() {
        let output = run_cargo_mdcat(["--help"]);
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(
            output.status.success(),
            "non-zero exit code: {:?}",
            output.status,
        );
        assert!(output.stderr.is_empty());
        assert!(stdout.contains("See 'man 1 mdcat' for more information."));
        assert!(stdout.contains("--show-frontmatter"));
    }

    #[test]
    fn long_version_includes_license() {
        let output = run_cargo_mdcat(["--version"]);
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(
            output.status.success(),
            "non-zero exit code: {:?}",
            output.status,
        );
        assert!(output.stderr.is_empty());
        assert!(
            stdout.contains("This program is subject to the terms of the Mozilla Public License,")
        );
    }

    #[test]
    fn file_list_fail_late() {
        let output = run_cargo_mdcat(["does-not-exist", "sample/common-mark.md"]);
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(!output.status.success());
        // We failed to read the first file but still printed the second.
        assert!(
            stderr.contains("Error: does-not-exist:") && stderr.contains("(os error 2)"),
            "Stderr: {stderr}",
        );
        assert!(stdout.contains("CommonMark sample document"));
    }

    #[test]
    fn file_list_fail_fast() {
        let output = run_cargo_mdcat(["--fail", "does-not-exist", "sample/common-mark.md"]);
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        assert!(!output.status.success());
        // We failed to read the first file and exited early, so nothing was printed at all
        assert!(
            stderr.contains("Error: does-not-exist:") && stderr.contains("(os error 2)"),
            "Stderr: {stderr}",
        );
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn toc_lists_headings_before_content() {
        let output = run_cargo_mdcat(["--no-colour", "--toc", "sample/common-mark.md"]);
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        let toc_pos = stdout
            .find("Table of Contents")
            .expect("TOC heading missing");
        let content_pos = stdout
            .find("CommonMark sample document")
            .expect("document content missing");
        assert!(
            toc_pos < content_pos,
            "TOC must come before the document content"
        );
        assert!(stdout.contains("common-mark.md#basic-inline-formatting"));
    }

    #[test]
    fn toc_on_stdin_has_no_links() {
        let mut child = cargo_mdcat()
            .args(["--no-colour", "--toc", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "# One\n\n# Two\n").unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(stdout.contains("Table of Contents"));
        assert!(!stdout.contains(".md#"));
    }

    #[test]
    fn tabs_flag_expands_tabs_in_code_blocks() {
        let mut child = cargo_mdcat()
            .args(["--no-colour", "--tabs", "4", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "```\na\tb\n```\n").unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(
            !stdout.contains('\t'),
            "tabs should be expanded: {stdout:?}"
        );
        assert!(
            stdout.contains("a   b"),
            "expected tab-stop-aligned spaces: {stdout:?}"
        );
    }

    #[test]
    fn without_tabs_flag_tabs_pass_through_unchanged() {
        let mut child = cargo_mdcat()
            .args(["--no-colour", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "```\na\tb\n```\n").unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(stdout.contains('\t'), "tab should pass through: {stdout:?}");
    }

    fn render_stdin(input: &str, args: &[&str]) -> String {
        let mut child = cargo_mdcat()
            .args(args)
            .arg("-")
            .env_remove("NO_COLOR")
            .env("MDCAT_PAGER", "cat")
            .env(
                "XDG_CONFIG_HOME",
                std::env::temp_dir().join(format!("mdcat-no-config-{}", std::process::id())),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn frontmatter_hidden_by_default() {
        for input in [
            "---\ntitle: Secret\n---\n# Body\n",
            "+++\ntitle = 'Secret'\n+++\n# Body\n",
            "\u{feff}--- \r\ntitle: Secret\r\n...\t\r\n# Body\n",
        ] {
            similar_asserts::assert_eq!(
                render_stdin(input, &["--no-colour"]),
                render_stdin("# Body\n", &["--no-colour"])
            );
        }
    }

    #[test]
    fn show_frontmatter_matches_themed_code_blocks() {
        for (syntax, block) in [
            ("yaml", "---\ntitle: Example\n---\n"),
            ("yaml", "---\ntitle: Example\n...\n"),
            ("toml", "+++\ntitle = 'Example'\n+++\n"),
        ] {
            for args in [
                vec!["--no-colour"],
                vec!["--ansi", "--theme", "dracula"],
                vec![
                    "--ansi",
                    "--theme",
                    "solarized-light",
                    "--margin",
                    "--columns",
                    "30",
                ],
            ] {
                let input = format!("{block}# Body\n");
                let expected = render_stdin(&format!("```{syntax}\n{block}```\n# Body\n"), &args);
                let mut show_args = args;
                show_args.push("--show-frontmatter");
                similar_asserts::assert_eq!(render_stdin(&input, &show_args), expected);
            }
        }
    }

    #[test]
    fn show_frontmatter_handles_bom_crlf_whitespace_and_eof() {
        for (syntax, block) in [
            ("yaml", "--- \r\ntitle: Example\r\n...\t\r\n"),
            ("toml", "+++\t\r\ntitle = 'Example'\r\n+++ \r\n"),
            ("yaml", "---\ntitle: Example\n---"),
            ("toml", "+++\ntitle = 'Example'\n+++"),
        ] {
            for bom in ["", "\u{feff}"] {
                let shown = render_stdin(
                    &format!("{bom}{block}"),
                    &["--no-colour", "--show-frontmatter"],
                );
                assert!(!shown.contains('\u{feff}'));
                assert!(shown.contains("title"));
                let lines: Vec<_> = shown
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .collect();
                let expected: Vec<_> = block.lines().map(str::trim_end).collect();
                let actual: Vec<_> = lines.into_iter().map(str::trim).collect();
                assert_eq!(actual, expected, "syntax: {syntax}");
            }
        }
    }

    #[test]
    fn show_frontmatter_leaves_unrecognized_input_unchanged() {
        for input in [
            "# Body\n",
            "---\ntitle: Unterminated\n",
            "+++\ntitle = 'Wrong closer'\n---\n",
            "# Body\n\n---\ntitle: Later\n---\n",
            " ---\ntitle: Indented\n---\n",
        ] {
            similar_asserts::assert_eq!(
                render_stdin(input, &["--no-colour", "--show-frontmatter"]),
                render_stdin(input, &["--no-colour"])
            );
        }
    }

    #[test]
    fn show_frontmatter_is_literal_and_excluded_from_toc() {
        let block = "---\n# Metadata heading\ntext: \"*literal* -- ... :smile:\"\ncode: ```\nindent:\tvalue\n---\n";
        let body = "# Body\n\n\"body\" -- ... :smile:\n";
        let args = [
            "--no-colour",
            "--smart-punctuation",
            "--emoji",
            "--tabs",
            "4",
            "--margin",
        ];
        let expected = render_stdin(&format!("````yaml\n{block}````\n{body}"), &args);
        let mut show_args = args.to_vec();
        show_args.push("--show-frontmatter");
        let shown = render_stdin(&format!("{block}{body}"), &show_args);
        similar_asserts::assert_eq!(&shown, &expected);
        assert_eq!(shown.matches("Metadata heading").count(), 1);
        assert!(shown.contains("*literal* -- ... :smile:"));
        assert!(shown.contains("“body” – … 😄"));
        assert!(!shown.contains('\t'));
        show_args.push("--toc");
        let shown = render_stdin(&format!("{block}{body}"), &show_args);
        assert_eq!(shown.matches("Metadata heading").count(), 1);
        assert!(shown.find("Metadata heading").unwrap() < shown.find("Table of Contents").unwrap());
        let mut body_args = args.to_vec();
        body_args.push("--toc");
        similar_asserts::assert_eq!(
            &shown[shown.find("Table of Contents").unwrap()..],
            render_stdin(body, &body_args).trim_start()
        );
    }

    #[cfg(unix)]
    #[test]
    fn show_frontmatter_works_with_pagination() {
        let block = "+++\ntitle = 'Example'\n+++\n";
        similar_asserts::assert_eq!(
            render_stdin(
                &format!("{block}# Body\n"),
                &["--no-colour", "--paginate", "--show-frontmatter"]
            ),
            render_stdin(
                &format!("```toml\n{block}```\n# Body\n"),
                &["--no-colour", "--paginate"]
            )
        );
    }

    #[test]
    fn show_frontmatter_preserves_relative_resources() {
        let dir = std::env::temp_dir().join(format!(
            "mdcat-frontmatter-resources-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sample/rust-logo-128x128.png"),
            dir.join("image.png"),
        )
        .unwrap();
        let file = dir.join("document.md");
        let block = "---\ntitle: Example\n---\n";
        let body = "# Body\n\n[relative](other.md)\n\n![image](image.png)\n";
        let render = |input: &str, show: bool| {
            std::fs::write(&file, input).unwrap();
            let mut cmd = cargo_mdcat();
            cmd.args(["--ansi", "--theme", "dracula", "--image-protocol", "kitty"]);
            if show {
                cmd.arg("--show-frontmatter");
            }
            let output = cmd.arg(&file).env_remove("NO_COLOR").output().unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        };
        let shown = render(&format!("{block}{body}"), true);
        let expected = render(&format!("```yaml\n{block}```\n{body}"), false);
        std::fs::remove_dir_all(&dir).unwrap();
        similar_asserts::assert_eq!(&shown, &expected);
        assert!(shown.contains("\x1b_G"), "relative image must still load");
        assert!(shown.contains("other.md"));
    }

    #[test]
    fn show_frontmatter_parses_with_watch() {
        let output = run_cargo_mdcat(["--watch", "--show-frontmatter", "sample/common-mark.md"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("--watch requires standard output to be a terminal"));
    }

    fn image_markdown() -> String {
        let image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sample")
            .join("rust-logo-128x128.png");
        // Embed a proper `file://` URL rather than a native path: on Windows, a bare
        // `D:\...` path isn't valid relative-reference syntax, so resolving it as a
        // Markdown link destination fails and the image silently falls back to a
        // plain link instead of actually rendering.
        let url = url::Url::from_file_path(&image).expect("absolute path");
        format!("![alt]({url})\n")
    }

    #[test]
    fn image_protocol_flag_forces_kitty_escape_sequence() {
        let mut child = cargo_mdcat()
            .args(["--ansi", "--image-protocol=kitty", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(
            stdout.contains("\x1b_G"),
            "expected a kitty graphics escape sequence, got: {stdout:?}"
        );
    }

    #[test]
    fn image_protocol_env_var_forces_iterm2_escape_sequence() {
        let mut child = cargo_mdcat()
            .args(["--ansi", "-"])
            .env("MDCAT_IMAGE_PROTOCOL", "iterm2")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(
            stdout.contains("\x1b]1337;File="),
            "expected an iTerm2 inline image escape sequence, got: {stdout:?}"
        );
    }

    #[test]
    fn image_protocol_flag_overrides_env_var() {
        let mut child = cargo_mdcat()
            .args(["--ansi", "--image-protocol=kitty", "-"])
            .env("MDCAT_IMAGE_PROTOCOL", "none")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(
            stdout.contains("\x1b_G"),
            "explicit --image-protocol should win over $MDCAT_IMAGE_PROTOCOL, got: {stdout:?}"
        );
    }

    #[test]
    fn image_protocol_flag_has_no_effect_when_paginating() {
        let mut child = cargo_mdcat()
            .args(["--paginate", "--image-protocol=kitty", "-"])
            .env("MDCAT_PAGER", "cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(output.status.success());
        assert!(
            !stdout.contains("\x1b_G"),
            "paginated output should not contain a kitty graphics escape sequence when the \
             pager is a plain one that can't handle it (see GH-45), got: {stdout:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn image_protocol_flag_still_applies_when_paginating_with_lessi() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "mdcat-cli-test-lessi-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let lessi_path = dir.join("lessi");
        std::fs::write(&lessi_path, "#!/bin/sh\ncat\n").unwrap();
        std::fs::set_permissions(&lessi_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut child = cargo_mdcat()
            .args(["--paginate", "--image-protocol=kitty", "-"])
            .env("MDCAT_PAGER", &lessi_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(output.status.success());
        assert!(
            stdout.contains("\x1b_G"),
            "lessi handles image escapes, so --image-protocol should still apply when \
             paginating through it, got: {stdout:?}"
        );
    }

    #[test]
    fn image_protocol_config_default_is_used_and_overridable() {
        let config_dir = std::env::temp_dir().join(format!(
            "mdcat-cli-test-config-{}-{}",
            std::process::id(),
            "image_protocol_config_default_is_used_and_overridable"
        ));
        std::fs::create_dir_all(config_dir.join("mdcat")).unwrap();
        std::fs::write(
            config_dir.join("mdcat/config.toml"),
            "[defaults]\nimage_protocol = \"kitty\"\n",
        )
        .unwrap();

        let run = |extra_args: &[&str]| {
            let mut child = cargo_mdcat()
                .args(["--ansi"])
                .args(extra_args)
                .arg("-")
                .env("XDG_CONFIG_HOME", &config_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            write!(child.stdin.take().unwrap(), "{}", image_markdown()).unwrap();
            let output = child.wait_with_output().unwrap();
            String::from_utf8(output.stdout).unwrap()
        };

        let from_config = run(&[]);
        assert!(
            from_config.contains("\x1b_G"),
            "config default should force kitty, got: {from_config:?}"
        );

        let overridden = run(&["--image-protocol=none"]);
        assert!(
            !overridden.contains("\x1b_G"),
            "explicit flag should override the config default, got: {overridden:?}"
        );

        std::fs::remove_dir_all(&config_dir).unwrap();
    }

    #[test]
    fn tabs_config_default_is_used_and_overridable() {
        let config_dir = std::env::temp_dir().join(format!(
            "mdcat-cli-test-config-{}-{}",
            std::process::id(),
            "tabs_config_default_is_used_and_overridable"
        ));
        std::fs::create_dir_all(config_dir.join("mdcat")).unwrap();
        std::fs::write(
            config_dir.join("mdcat/config.toml"),
            "[defaults]\ntabs = 4\n",
        )
        .unwrap();

        let run = |extra_args: &[&str]| {
            let mut child = cargo_mdcat()
                .args(["--no-colour"])
                .args(extra_args)
                .arg("-")
                .env("XDG_CONFIG_HOME", &config_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            write!(child.stdin.take().unwrap(), "```\na\tb\n```\n").unwrap();
            let output = child.wait_with_output().unwrap();
            String::from_utf8(output.stdout).unwrap()
        };

        let from_config = run(&[]);
        assert!(
            !from_config.contains('\t'),
            "config default should expand tabs, got: {from_config:?}"
        );

        std::fs::remove_dir_all(&config_dir).unwrap();
    }

    #[test]
    fn ignore_broken_pipe() {
        let mut child = cargo_mdcat()
            .stdin(Stdio::piped())
            // .arg("sample/common-mark.md")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdin = child.stdin.take().unwrap();
        let mut stderr = Vec::new();
        drop(child.stdout.take());

        writeln!(stdin, "Hello world").unwrap();
        drop(stdin);
        child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_end(&mut stderr)
            .unwrap();
        let exit_code = child.wait().unwrap();

        similar_asserts::assert_eq!(String::from_utf8_lossy(&stderr), "");
        assert_eq!(exit_code.code().unwrap(), 0);
    }
}

/// Exercises `mdpick`, i.e. the multicall entry point invoked as `mdpick`.
///
/// Unix-only: relies on symlinks to fake `argv[0]` and on a `sh` script standing in for `fzf`.
#[cfg(unix)]
mod mdpick {
    use std::os::unix::fs::symlink;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Output};

    /// A scratch directory holding a `mdpick` symlink to the test binary and a stub `fzf`,
    /// cleaned up on drop.
    struct Sandbox {
        dir: std::path::PathBuf,
    }

    impl Sandbox {
        /// Set up a sandbox whose stub `fzf` runs `fzf_script` (a `sh` script body) and exposes
        /// `mdpick` on `$PATH`.
        fn new(fzf_script: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "mdcat-mdpick-test-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            symlink(env!("CARGO_BIN_EXE_mdcat"), dir.join("mdpick")).unwrap();
            let fzf_path = dir.join("fzf");
            std::fs::write(&fzf_path, format!("#!/bin/sh\n{fzf_script}\n")).unwrap();
            std::fs::set_permissions(&fzf_path, std::fs::Permissions::from_mode(0o755)).unwrap();
            Sandbox { dir }
        }

        fn run<I, S>(&self, args: I) -> Output
        where
            I: IntoIterator<Item = S>,
            S: AsRef<std::ffi::OsStr>,
        {
            let path = format!("{}:{}", self.dir.display(), std::env::var("PATH").unwrap());
            Command::new(self.dir.join("mdpick"))
                .args(args)
                .env("PATH", path)
                .output()
                .unwrap()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn renders_the_file_selected_in_fzf() {
        let sandbox = Sandbox::new("grep -m1 math");
        let output = sandbox.run(["sample", "--no-colour"]);
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        assert!(
            output.status.success(),
            "non-zero exit code: {:?}, stderr: {}",
            output.status,
            std::str::from_utf8(&output.stderr).unwrap(),
        );
        assert!(stdout.contains("Math rendering"), "stdout: {stdout}");
    }

    #[test]
    fn exits_cleanly_when_the_picker_is_cancelled() {
        let sandbox = Sandbox::new("cat >/dev/null; exit 130");
        let output = sandbox.run(["sample"]);
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn errors_when_the_directory_has_no_markdown_files() {
        let sandbox = Sandbox::new("cat >/dev/null; echo should-not-be-selected");
        let empty = sandbox.dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let output = sandbox.run([empty.to_str().unwrap()]);
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        assert!(!output.status.success());
        assert!(stderr.contains("No Markdown files found"), "{stderr}");
    }

    #[test]
    fn errors_with_more_than_one_directory_argument() {
        let sandbox = Sandbox::new("cat >/dev/null");
        let output = sandbox.run(["sample", "sample2"]);
        assert!(!output.status.success());
        assert!(std::str::from_utf8(&output.stderr)
            .unwrap()
            .contains("at most one directory"));
    }
}
