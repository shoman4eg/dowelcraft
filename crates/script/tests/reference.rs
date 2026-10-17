//! Прогон скриптов пользователей (reference/scripts) без окна на «Кухне 24.04»: сколько выполняется
//! без ошибок. Число успешных не должно падать (защита от регрессий пролога).

use std::path::Path;

#[test]
fn reference_scripts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("reference/scripts");
    let model = root.join("reference/pairs/kuhnya-24-04/_Кухня 24.04.b3d");
    let (Ok(rd), Ok(job)) = (std::fs::read_dir(&dir), basis::job::Job::open(&model)) else { return };
    let project = job.import();
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "js")).collect();
    files.sort();
    let sandbox = std::env::temp_dir().join(format!("basis-craft-ref-{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let (mut ok, mut errors) = (0, std::collections::BTreeMap::<String, Vec<String>>::new());
    for f in &files {
        let src = script::read_script(f).unwrap();
        let mut s = basis::design::Session::new(project.clone());
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
    eprintln!("выполнено без ошибок: {ok} из {}", files.len());
    assert!(ok >= MIN_OK, "регрессия: {ok} < {MIN_OK}");
}

/// Нижняя граница успешных скриптов (поднимается по мере развития пролога).
const MIN_OK: usize = 81;
