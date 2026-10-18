//! Runs user scripts (reference/scripts) without a window on the "Kitchen 24.04" sample: how many run
//! without errors. The success count must not drop (guards against prelude regressions).

use std::path::Path;

#[test]
fn reference_scripts() {
    // The JS engine is recursive: in a debug build 2 MB of test-thread stack is not enough for deep scripts
    // (in the window the script runs on the main thread, which has 8 MB).
    std::thread::Builder::new().stack_size(64 << 20).spawn(run_all).unwrap().join().unwrap();
}

fn run_all() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("reference/scripts");
    // The kitchen model of the local sample pair (the only .b3d in its folder).
    let pair = root.join("reference/pairs/kuhnya-24-04");
    let model = std::fs::read_dir(&pair).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "b3d"));
    let (Ok(rd), Some(Ok(job))) = (std::fs::read_dir(&dir), model.map(|m| dowel::job::Job::open(&m))) else { return };
    let project = job.import();
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "js")).collect();
    files.sort();
    let sandbox = std::env::temp_dir().join(format!("dowel-craft-ref-{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let (mut ok, mut errors) = (0, std::collections::BTreeMap::<String, Vec<String>>::new());
    for f in &files {
        let src = script::read_script(f).unwrap();
        let mut s = dowel::design::Session::new(project.clone());
        let opts = script::Options { sandbox: sandbox.clone(), script_dir: dir.clone(), selection: vec![(1, 1, 0), (2, 2, 0)], loop_limit: 20_000_000 };
        let r = script::run(&mut s, &src, opts);
        match r.error {
            None => ok += 1,
            Some(e) => {
                let key: String = e.lines().next().unwrap_or("").chars().take(90).collect();
                errors.entry(key).or_default().push(f.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut by: Vec<_> = errors.into_iter().collect();
    by.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (e, v) in &by {
        eprintln!("{:3}  {e}  [{}]", v.len(), v.iter().take(2).cloned().collect::<Vec<_>>().join("; "));
    }
    eprintln!("ran without errors: {ok} of {}", files.len());
    assert!(ok >= MIN_OK, "regression: {ok} < {MIN_OK}");
}

/// Lower bound of successful scripts (raised as the prelude develops).
const MIN_OK: usize = 81;
