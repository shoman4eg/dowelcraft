use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dowel::b3d::{self, Node};
use dowel::design::{self, Project, Session};
use dowel::job::{self, Job};
use dowel::nest::Params;
use dowel::panel;
use dowel::i18n::t;
use dowel::report;

const USAGE: &str = "dowel-craft — cabinet furniture design and sheet cutting; reads Bazis-Mebelshchik models

  dowel-craft [file]                                  the window (with a file — open it)
  dowel-craft mcp                                     MCP server for AI (stdio)
  dowel-craft --version                               version
  --lang ru (or DOWEL_LANG=ru)                        Russian messages and documents

A model is a Bazis .b3d file or your own .dcraft project:
  dowel-craft spec    <model> [-o out.csv]            parts list
  dowel-craft cut     <model> [-o maps.html] [--sheet 2800x2070] [--kerf 4] [--trim 10]
                                                     sheet cutting: maps and “Panels for production” (HTML to print)
  dowel-craft smeta   <model> [-o smeta.csv] [--coef 1.2]
                                                     board (m²) and edge banding (m) usage, like the Bazis estimate
  dowel-craft raskroy <model> [-o project.xml] [--sheet 2800x2070]
                                                     a BAZIS-Raskroy project (XML)
  dowel-craft dump    <file.b3d> [depth]              the Bazis model tree

Your own project:
  dowel-craft import <file.b3d> [-o project.dcraft]  Bazis model → your project (parts become editable)
  dowel-craft b3d <project.dcraft> [-o model.b3d]     your project → a Bazis-Mebelshchik model
  dowel-craft obj <model> [-o model.obj]              3D model for visualisation (OBJ + MTL, mm)
  dowel-craft script <project.dcraft> <script.js>     run a Bazis script on the project and save it
  dowel-craft check <model>                           model check: overlaps, fasteners, sizes
  dowel-craft drawings <model> [-o drawings.html|.pdf] assembly and part drawings (A4; .pdf without a browser)
  dowel-craft design <project.dcraft> <command> ['{json parameters}']
                                                     run a command; the file is created if missing
  dowel-craft design --help                          the list of commands

Sheet size and factor come from a .dcraft project unless --sheet and --coef are given.";

/// The usage text in the current language, line by line (`lang/ru.tsv` has each line).
fn usage() -> String {
    USAGE.lines().map(t).collect::<Vec<_>>().join("\n")
}

/// Commands handled here (the first argument of `dowel-craft`); anything else opens the window.
pub fn is_command(name: &str) -> bool {
    matches!(name, "spec" | "cut" | "smeta" | "raskroy" | "dump" | "import" | "b3d" | "obj" | "script" | "check" | "drawings" | "design" | "help" | "--help" | "-h")
}

/// Run a command: `args` without the program name.
pub fn main(args: &[String]) -> ExitCode {
    if matches!(args.first().map(String::as_str), Some("help" | "--help" | "-h")) {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", t(format!("error: {e}")));
            ExitCode::FAILURE
        }
    }
}

fn opt<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn num(args: &[String], key: &str) -> Result<Option<f64>, String> {
    opt(args, key).map(|v| v.replace(',', ".").parse().map_err(|_| t(format!("{key}: not a number “{v}”")))).transpose()
}

fn sheet_arg(args: &[String]) -> Result<Option<(f64, f64)>, String> {
    let Some(s) = opt(args, "--sheet") else { return Ok(None) };
    // Bazis users may type the Russian letter for "x".
    let s = s.replace(dowel::i18n::bazis("times"), "x");
    let (a, b) = s.split_once(['x', '*']).ok_or_else(|| t("--sheet: LENGTHxWIDTH is required"))?;
    Ok(Some((a.parse().map_err(|_| t("--sheet: length"))?, b.parse().map_err(|_| t("--sheet: width"))?)))
}

const DEFAULT_SHEET: (f64, f64) = (2800.0, 2070.0);

fn write_or_print(args: &[String], text: &str) -> Result<(), String> {
    match opt(args, "-o") {
        Some(out) => dowel::write(out, text).map_err(|e| format!("{out}: {e}")),
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
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("dcraft"));
            dowel::write(&out, p.save()).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("{}", t(format!("parts {}, materials {} → {}", p.panels().len(), p.materials.len(), out.display())));
            return Ok(());
        }
        "b3d" => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let project = dowel::design::Project::load(&text)?;
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("b3d"));
            dowel::write(&out, dowel::export::b3d(&project)?).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("{}", t(format!("parts {} → {}", dowel::design::build(&project)?.len(), out.display())));
            return Ok(());
        }
        "script" => {
            let js = args.get(2).ok_or_else(|| t("a script file is required"))?;
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let mut s = dowel::design::Session::new(dowel::design::Project::load(&text)?);
            let js_path = Path::new(js);
            let dir = js_path.parent().map(PathBuf::from).unwrap_or_default();
            let r = script::run(&mut s, &script::read_script(js_path)?, script::Options { sandbox: dir.clone(), script_dir: dir, ..Default::default() });
            r.log.iter().for_each(|l| println!("{l}"));
            if let Some(e) = r.error {
                return Err(t(format!("script: {e} (the project is unchanged)")));
            }
            if r.changes > 0 {
                dowel::write(path, s.project.save()).map_err(|e| format!("{path}: {e}"))?;
            }
            println!("{}", t(format!("changes: {}, dialogs: {}", r.changes, r.forms.len())));
            return Ok(());
        }
        "obj" => {
            let job = Job::open(Path::new(path))?;
            let parts = dowel::job::parts(&job)?;
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("obj"));
            dowel::obj::save(&out, &parts, job.project.as_ref(), &job.name)?;
            println!("{}", t(format!("parts {} → {}", parts.len(), out.display())));
            return Ok(());
        }
        "drawings" => {
            let job = Job::open(Path::new(path))?;
            let parts = dowel::job::parts(&job)?;
            let pages = dowel::drawing::drawings(&parts, &job.name);
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("drawings.html"));
            let data = if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) { dowel::pdf::from_svg_pages(&pages)? } else { report::document_html(&t(format!("{} — drawings", job.name)), &pages, "").into_bytes() };
            dowel::write(&out, data).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("{}", t(format!("sheets {} → {}", parts.len().min(1) + dowel::panel::spec(&job.panels).len(), out.display())));
            return Ok(());
        }
        "check" => {
            let job = Job::open(Path::new(path))?;
            let project = job.project.clone().unwrap_or_else(|| job.import());
            let problems = dowel::design::analysis::analyze(&project)?;
            for p in &problems {
                println!("{}: {}", p.kind, p.message);
            }
            println!("{}", t(format!("problems: {}", problems.len())));
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
        eprintln!("{}", t(format!("boards skipped, contour not understood: {} ({})", job.skipped.len(), job.skipped.join(", "))));
    }
    let sheet = sheet_arg(args)?;
    let sheet_for = |m: &str| sheet.unwrap_or_else(|| job.sheet(m, DEFAULT_SHEET));
    match cmd.as_str() {
        "spec" => write_or_print(args, &report::spec_csv(&panel::spec(&job.panels)))?,
        "smeta" => {
            let coef = num(args, "--coef")?;
            let rows = job.estimate_by(|m| coef.unwrap_or_else(|| job.coef(m, 1.2)));
            write_or_print(args, &report::materials_csv(&rows))?;
        }
        "raskroy" => {
            let xml = dowel::raskroy::project_xml(&job.name, &job.panels, sheet_for);
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("xml"));
            dowel::write(&out, xml).map_err(|e| format!("{}: {e}", out.display()))?;
            let holes: usize = job.panels.iter().map(|p| p.holes.len()).sum();
            println!("{}", t(format!("parts {}, drilling holes {holes} → {}", job.panels.len(), out.display())));
            if job.holes_lost > 0 {
                eprintln!("  {}", t(format!("fastener holes without a board (not exported): {}", job.holes_lost)));
            }
        }
        "cut" => {
            let (kerf, trim) = (num(args, "--kerf")?.unwrap_or(4.0), num(args, "--trim")?.unwrap_or(10.0));
            let cuts = job::cut(&job, |m| Params { sheet: sheet_for(m), ..job.params(m, Params { sheet: DEFAULT_SHEET, kerf, trim }) });
            for c in &cuts {
                let n: usize = c.layout.sheets.iter().map(|s| s.parts.len()).sum();
                println!("{}", t(format!("{}: parts {n}, sheets {}", panel::material_name(&c.material), c.layout.sheets.len())));
                for (k, s) in c.layout.sheets.iter().enumerate() {
                    println!("  {}", t(format!("map {}: {:.1}%", k + 1, s.usage(&c.params) * 100.0)));
                }
                for &id in &c.layout.too_big {
                    let (l, w) = job.panels[id].cut_size();
                    eprintln!("  {}", t(format!("does not fit a sheet: No. {} “{}” {l:.0}x{w:.0}", job.number(id), job.panels[id].name)));
                }
            }
            let out = opt(args, "-o").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_extension("html"));
            // .pdf — cutting maps as PDF (no browser), otherwise HTML with the maps and "Panels for production".
            let data = if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) { dowel::pdf::from_svg_pages(&job::cards(&job, &cuts))? } else { job::document(&job, &cuts).into_bytes() };
            dowel::write(&out, data).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("→ {}", out.display());
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// Command on a .dcraft project: load (or create), execute, save if changed.
fn design_cmd(path: &str, rest: &[String]) -> Result<(), String> {
    if path == "--help" {
        println!("{}", t("Project commands:"));
        for (name, help) in design::COMMANDS {
            println!("  {name:18} {}", t(help));
        }
        return Ok(());
    }
    let cmd = rest.first().ok_or_else(|| t("a command is required, see dowel-craft design --help"))?;
    let params: serde_json::Value = match rest.get(1) {
        Some(j) => serde_json::from_str(j).map_err(|e| t(format!("parameters: {e}")))?,
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
        dowel::write(path, s.project.save()).map_err(|e| format!("{path}: {e}"))?;
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
