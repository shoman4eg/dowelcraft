use super::*;
use dowel::design::Project;

fn session() -> Session {
    let mut s = Session::new(Project::new("Скрипт"));
    s.execute("material.add", &json!({"name": "ЛДСП 16", "thickness": 16})).unwrap();
    s.execute("edge.add", &json!({"name": "ПВХ 2", "thickness": 2})).unwrap();
    s
}

fn opts() -> Options {
    let dir = std::env::temp_dir().join(format!("dowel-craft-script-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Options { sandbox: dir.clone(), script_dir: dir, ..Options::default() }
}

#[test]
fn old_api_creates_and_edits_panels() {
    let mut s = session();
    let r = run(&mut s, r#"
        var p = AddFrontPanel(0, 0, 600, 400, 0);
        p.Name = 'Задник';
        AddVertPanel(0, 0, 500, 700, 1000);
        var n = 0;
        Model.forEachPanel(function (x) { n++; system.log(x.Name, x.GSize.x, x.GSize.y, x.Thickness, x.MaterialName); });
        var c = p.Contour;
        alert('торцов', c.Count, c.Objects[0].IsLine(), Math.round(c.ObjLength()));
        p.AddButt('ПВХ 2', 0);
    "#, opts());
    assert_eq!(r.error, None, "{:?}", r.log);
    assert_eq!(r.log, vec!["Задник 600 400 16 ЛДСП 16", "Вертикальная 2 500 700 16 ЛДСП 16", "торцов 4 true 2000"]);
    let parts = design::build(&s.project).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].panel.edges.len(), 1);
    assert!(s.can_undo());
}

#[test]
fn errors_revert_changes_and_explain() {
    let mut s = session();
    let before = s.project.clone();
    let r = run(&mut s, "AddFrontPanel(0, 0, 600, 400, 0); var x = NewCOMObject('Excel.Application');", opts());
    assert!(r.error.as_deref().unwrap_or("").contains("Windows"), "{:?}", r.error);
    assert_eq!(s.project, before, "ошибка скрипта не меняет модель");
    let r = run(&mut s, "fs.writeFileSync('../../etc/x', 'a')", opts());
    assert!(r.error.as_deref().unwrap_or("").contains("выходит"), "{:?}", r.error);
    let r = run(&mut s, "while (true) {}", Options { loop_limit: 10_000, ..opts() });
    assert!(r.error.is_some(), "бесконечный цикл прерывается");
}

#[test]
fn files_ui_and_properties() {
    let mut s = session();
    let o = opts();
    let r = run(&mut s, r#"
        var fs = require('fs');
        fs.writeFileSync('отчёт.txt', 'привет');
        var t = fs.readFileSync('отчёт.txt');
        var f = NewForm(); f.Caption = 'Параметры';
        var b = NewButton(f); b.Caption = 'OK';
        f.Show();
        var w = Action.Properties.NewNumber('Ширина', 600);
        alert(t, w.Value, fs.existsSync('отчёт.txt'));
        Action.OnFinish = function () { alert('конец'); };
    "#, o.clone());
    assert_eq!(r.error, None);
    assert_eq!(r.log, vec!["привет 600 true", "конец"]);
    assert_eq!(r.forms.len(), 1);
    assert_eq!(r.forms[0]["Caption"], "Параметры");
    assert_eq!(r.forms[0]["Children"][0]["Caption"], "OK");
    assert!(o.sandbox.join("отчёт.txt").exists());
}

#[test]
fn modular_api() {
    let mut s = session();
    let r = run(&mut s, r#"
        materialData.SetupActiveFurnMaterial('ЛДСП Белый\rW980', 18);
        materialData.SetupActiveButtMaterial('Кромка ПВХ Бук 0,4/19', 0.4, 19);
        let panel = objects3d.NewPanel(100, 200, objects3d.PanelOrientation.vertical);
        panel.Name = 'Вертикальная';
        panelOperations.AddButt(panel, 0);
    "#, opts());
    assert_eq!(r.error, None, "{:?}", r.log);
    let parts = design::build(&s.project).unwrap();
    assert_eq!((parts[0].panel.name.as_str(), parts[0].panel.thickness, parts[0].panel.edges.len()), ("Вертикальная", 18.0, 1));
}

#[test]
fn live_form_buttons_call_script() {
    let mut s = session();
    let mut rt = Runtime::start(&s, r#"
        var f = NewForm(); f.Caption = 'Полка';
        var w = NewNumber(f); w.Caption = 'Ширина'; w.Value = 500;
        var b = NewButton(f); b.Caption = 'Поставить';
        b.OnClick = function () { AddHorizPanel(0, 0, w.Value, 400, 700); f.Close(); };
        f.Show();
    "#, opts());
    assert_eq!(rt.error, None);
    let forms = rt.forms();
    assert_eq!(forms.len(), 1);
    let kids = forms[0]["Children"].as_array().unwrap();
    let (num, btn) = (kids[0]["__id"].as_i64().unwrap(), kids[1]["__id"].as_i64().unwrap());
    rt.change(num, "Value", &json!(640));
    rt.click(btn);
    assert!(rt.forms().is_empty(), "форма закрылась");
    let r = rt.finish(&mut s);
    assert_eq!(r.error, None);
    let parts = design::build(&s.project).unwrap();
    assert_eq!(parts[0].panel.size, (640.0, 400.0));
}
