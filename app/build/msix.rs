//! The Microsoft Store package's layout (docs/plans/store.md): `AppxManifest.xml` from its template, with the
//! version filled in, the logos, drawn from the app glyph like the exe's icon, and the manifest's translated
//! strings (`Strings/<language>/Resources.resw`). `tools/pack-msix.ps1` adds the exe and the license files,
//! indexes the logos and strings (`resources.pri`) and packs it.

use crate::{icon, png};
use std::path::Path;

/// Taskbar, Start's list and search ask for these exact sizes; `altform-unplated` is the variant shown without
/// the accent plate, which the tile, with its own background, is meant for.
const TARGET_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 96, 256];
/// Display scales, in percent.
const SCALES: [u32; 5] = [100, 125, 150, 200, 400];

/// (file name under `Assets`, PNG) for every logo the manifest names, at every size Windows picks from.
fn logos() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut add =
        |name: String, canvas: u32, size: u32| out.push((name, png::encode(canvas, &icon::draw_in(canvas, size))));
    for t in TARGET_SIZES {
        add(format!("Square44x44Logo.targetsize-{t}.png"), t, t);
        add(format!("Square44x44Logo.targetsize-{t}_altform-unplated.png"), t, t);
    }
    for s in SCALES {
        let px = |base: u32| (base * s).div_ceil(100);
        add(format!("Square44x44Logo.scale-{s}.png"), px(44), px(44));
        // Start's medium tile: the glyph's tile at half the square, as Windows' own apps leave room around theirs.
        add(format!("Square150x150Logo.scale-{s}.png"), px(150), px(75));
        add(format!("StoreLogo.scale-{s}.png"), px(50), px(50));
    }
    out
}

/// The app's description (manifest `ms-resource:AppDescription`, shown by Start and Settings → Apps) in each
/// language the manifest lists under `Resources`, by its tag, the default first. The name stays "busy — system
/// monitor" in every language: a translated package name would have to be reserved in Partner Center too.
const DESCRIPTIONS: [(&str, &str); 11] = [
    ("en-US", "Live CPU, memory, GPU, network, disk, battery and sensor readings in the taskbar."),
    ("de-DE", "Live-Werte von CPU, Arbeitsspeicher, GPU, Netzwerk, Datenträger, Akku und Sensoren in der Taskleiste."),
    ("es-ES", "Lecturas en directo de CPU, memoria, GPU, red, disco, batería y sensores en la barra de tareas."),
    (
        "fr-FR",
        "Mesures en direct du processeur, de la mémoire, du GPU, du réseau, du disque, de la batterie et des capteurs dans la barre des tâches.",
    ),
    (
        "it-IT",
        "Letture in tempo reale di CPU, memoria, GPU, rete, disco, batteria e sensori nella barra delle applicazioni.",
    ),
    (
        "ja-JP",
        "CPU、メモリ、GPU、ネットワーク、ディスク、バッテリー、センサーの値をタスク バーにリアルタイムで表示します。",
    ),
    ("ko-KR", "CPU, 메모리, GPU, 네트워크, 디스크, 배터리, 센서 값을 작업 표시줄에 실시간으로 표시합니다."),
    ("pl-PL", "Bieżące odczyty procesora, pamięci, GPU, sieci, dysku, baterii i czujników na pasku zadań."),
    ("pt-BR", "Leituras ao vivo de CPU, memória, GPU, rede, disco, bateria e sensores na barra de tarefas."),
    ("ru-RU", "Показания ЦП, памяти, GPU, сети, диска, батареи и датчиков на панели задач в реальном времени."),
    ("zh-CN", "在任务栏中实时显示 CPU、内存、GPU、网络、磁盘、电池和传感器读数。"),
];

/// A `.resw` file (the ResX format makepri indexes) holding one string.
fn resw(name: &str, value: &str) -> String {
    let value = value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root>
  <resheader name="resmimetype"><value>text/microsoft-resx</value></resheader>
  <resheader name="version"><value>2.0</value></resheader>
  <data name="{name}" xml:space="preserve"><value>{value}</value></data>
</root>
"#
    )
}

/// Writes the layout under `dir`.
pub fn write(dir: &Path, template: &str, version: &str) -> std::io::Result<()> {
    let assets = dir.join("Assets");
    std::fs::create_dir_all(&assets)?;
    std::fs::write(dir.join("AppxManifest.xml"), template.replace("@VERSION@", version))?;
    for (name, data) in logos() {
        std::fs::write(assets.join(name), data)?;
    }
    for (lang, text) in DESCRIPTIONS {
        let strings = dir.join("Strings").join(lang);
        std::fs::create_dir_all(&strings)?;
        std::fs::write(strings.join("Resources.resw"), resw("AppDescription", text))?;
    }
    Ok(())
}
