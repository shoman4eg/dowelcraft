use std::path::{Path, PathBuf};
use std::process::ExitCode;

use basis::b3d::{self, Node};
use basis::design::{self, Project, Session};
use basis::job::{self, Job};
use basis::nest::Params;
use basis::panel;
use basis::report;

const USAGE: &str = "basis-craft — проектирование мебели и раскрой; читает модели Базис-Мебельщика

Модель — файл Базиса .b3d или свой проект .bcraft:
  basis-craft spec    <модель> [-o out.csv]           спецификация деталей
  basis-craft cut     <модель> [-o карты.html] [--sheet 2800x2070] [--kerf 4] [--trim 10]
                                                     раскрой: карты и «Панели в производство» (HTML для печати)
  basis-craft smeta   <модель> [-o smeta.csv] [--coef 1.2]
                                                     расход плит (м²) и кромки (м), как смета Базиса
  basis-craft raskroy <модель> [-o проект.xml] [--sheet 2800x2070]
                                                     проект для БАЗИС-Раскроя (XML)
  basis-craft dump    <файл.b3d> [глубина]            дерево модели Базиса

Свой проект:
  basis-craft import <файл.b3d> [-o проект.bcraft]   модель Базиса → свой проект (детали можно править)
  basis-craft b3d <проект.bcraft> [-o модель.b3d]     свой проект → модель Базис-Мебельщика
  basis-craft script <проект.bcraft> <скрипт.js>      выполнить скрипт Базиса над проектом и сохранить
  basis-craft design <проект.bcraft> <команда> ['{json-параметры}']
                                                     выполнить команду; файл создаётся, если его нет
  basis-craft design --help                          список команд

Формат листа и коэффициент из проекта .bcraft используются, если не заданы --sheet и --coef.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e}");
            ExitCode::FAILURE
        }
    }
}

fn opt<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn num(args: &[String], key: &str) -> Result<Option<f64>, String> {
    opt(args, key).map(|v| v.replace(',', ".").parse().map_err(|_| format!("{key}: не число «{v}»"))).transpose()
}

fn sheet_arg(args: &[String]) -> Result<Option<(f64, f64)>, String> {
    let Some(s) = opt(args, "--sheet") else { return Ok(None) };
    let (a, b) = s.split_once(['x', 'х', '*']).ok_or("--sheet: нужно ДЛИНАxШИРИНА")?;
    Ok(Some((a.parse().map_err(|_| "--sheet: длина")?, b.parse().map_err(|_| "--sheet: ширина")?)))
}

const DEFAULT_SHEET: (f64, f64) = (2800.0, 2070.0);

fn write_or_print(args: &[String], text: &str) -> Result<(), String> {
    match opt(args, "-o") {
        Some(out) => std::fs::write(out, text).map_err(|e| format!("{out}: {e}")),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (Some(cmd), Some(path)) = (args.first(), args.get(1)) else {
        return Err(USAGE.into());
    };
    match cmd.as_str() {
        "design" => return design_cmd(path, &args[2..]),
        "import" => {
            let job = Job::open(Path::new(path))?;
            let p = job.import();
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("bcraft"));
            std::fs::write(&out, p.save()).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("деталей {}, материалов {} → {}", p.panels().len(), p.materials.len(), out.display());
            return Ok(());
        }
        "b3d" => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let project = basis::design::Project::load(&text)?;
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("b3d"));
            std::fs::write(&out, basis::export::b3d(&project)?).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("деталей {} → {}", basis::design::build(&project)?.len(), out.display());
            return Ok(());
        }
        "script" => {
            let js = args.get(2).ok_or("нужен файл скрипта")?;
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let mut s = basis::design::Session::new(basis::design::Project::load(&text)?);
            let js_path = Path::new(js);
            let dir = js_path.parent().map(PathBuf::from).unwrap_or_default();
            let r = script::run(&mut s, &script::read_script(js_path)?, script::Options { sandbox: dir.clone(), script_dir: dir, ..Default::default() });
            r.log.iter().for_each(|l| println!("{l}"));
            if let Some(e) = r.error {
                return Err(format!("скрипт: {e} (проект не изменён)"));
            }
            if r.changes > 0 {
                std::fs::write(path, s.project.save()).map_err(|e| format!("{path}: {e}"))?;
            }
            println!("изменений: {}, окон: {}", r.changes, r.forms.len());
            return Ok(());
        }
        "dump" => {
            let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let file = b3d::parse(&data).map_err(|e| format!("{path}: {e}"))?;
            let depth = args.get(2).and_then(|d| d.parse().ok()).unwrap_or(usize::MAX);
            for s in &file.sections {
                dump(s, 0, depth);
            }
            return Ok(());
        }
        _ => {}
    }
    let job = Job::open(Path::new(path))?;
    if !job.skipped.is_empty() {
        eprintln!("пропущено плит с неразобранным контуром: {} ({})", job.skipped.len(), job.skipped.join(", "));
    }
    let sheet = sheet_arg(args)?;
    let sheet_for = |m: &str| sheet.unwrap_or_else(|| job.sheet(m, DEFAULT_SHEET));
    match cmd.as_str() {
        "spec" => write_or_print(args, &report::spec_csv(&panel::spec(&job.panels)))?,
        "smeta" => {
            let coef = num(args, "--coef")?;
            let mut rows = panel::materials(&job.panels, |m| coef.unwrap_or_else(|| job.coef(m, 1.2)));
            rows.extend(job.estimate(1.2).into_iter().filter(|r| r.unit == "шт"));
            write_or_print(args, &report::materials_csv(&rows))?;
        }
        "raskroy" => {
            let xml = basis::raskroy::project_xml(&job.name, &job.panels, sheet_for);
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("xml"));
            std::fs::write(&out, xml).map_err(|e| format!("{}: {e}", out.display()))?;
            let holes: usize = job.panels.iter().map(|p| p.holes.len()).sum();
            println!("деталей {}, отверстий присадки {holes} → {}", job.panels.len(), out.display());
            if job.holes_lost > 0 {
                eprintln!("  отверстий крепежа без плиты (не попали в выгрузку): {}", job.holes_lost);
            }
        }
        "cut" => {
            let (kerf, trim) = (num(args, "--kerf")?.unwrap_or(4.0), num(args, "--trim")?.unwrap_or(10.0));
            let cuts = job::cut(&job, |m| Params { sheet: sheet_for(m), ..job.params(m, Params { sheet: DEFAULT_SHEET, kerf, trim }) });
            for c in &cuts {
                let n: usize = c.layout.sheets.iter().map(|s| s.parts.len()).sum();
                println!("{}: деталей {n}, листов {}", panel::material_name(&c.material), c.layout.sheets.len());
                for (k, s) in c.layout.sheets.iter().enumerate() {
                    println!("  карта {}: {:.1}%", k + 1, s.usage(&c.params) * 100.0);
                }
                for &id in &c.layout.too_big {
                    let (l, w) = job.panels[id].cut_size();
                    eprintln!("  не помещается на лист: № {} «{}» {l:.0}x{w:.0}", job.number(id), job.panels[id].name);
                }
            }
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("html"));
            std::fs::write(&out, job::document(&job, &cuts)).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("→ {}", out.display());
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// Команда над проектом .bcraft: загрузить (или создать), выполнить, сохранить при изменении.
fn design_cmd(path: &str, rest: &[String]) -> Result<(), String> {
    if path == "--help" {
        println!("Команды проекта:");
        for (name, help) in design::COMMANDS {
            println!("  {name:18} {help}");
        }
        return Ok(());
    }
    let cmd = rest.first().ok_or("нужна команда, см. basis-craft design --help")?;
    let params: serde_json::Value = match rest.get(1) {
        Some(j) => serde_json::from_str(j).map_err(|e| format!("параметры: {e}"))?,
        None => serde_json::json!({}),
    };
    let project = match std::fs::read_to_string(path) {
        Ok(text) => Project::load(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Project::new(&Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
        }
        Err(e) => return Err(format!("{path}: {e}")),
    };
    let mut s = Session::new(project);
    let before = s.project.clone();
    let out = s.execute(cmd, &params)?;
    if s.project != before {
        std::fs::write(path, s.project.save()).map_err(|e| format!("{path}: {e}"))?;
    }
    if !out.is_null() {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    }
    Ok(())
}

fn dump(n: &Node, depth: usize, max: usize) {
    if depth > max {
        return;
    }
    println!("{}{} {}", "  ".repeat(depth), n.name, panel::describe(&n.value));
    for c in &n.children {
        dump(c, depth + 1, max);
    }
}
