//! LibreHardwareMonitor / OpenHardwareMonitor WMI provider (`root\LibreHardwareMonitor` / `root\OpenHardwareMonitor`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use busy_core::{SensorKind, SensorReading};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_R4, VT_R8, VariantClear};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_CONNECT_USE_MAX_WAIT, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WbemLocator,
};
use windows::core::{BSTR, PCWSTR, Result, w};

use crate::reading;

const RECONNECT: Duration = Duration::from_secs(30);
const HW_REFRESH: Duration = Duration::from_secs(60);
/// Queries slower than this are throttled to `SLOW_INTERVAL`, reusing the last values in between.
const SLOW_QUERY: Duration = Duration::from_millis(15);
const SLOW_INTERVAL: Duration = Duration::from_secs(3);
const NAMESPACES: [(&str, &str); 2] =
    [(r"root\LibreHardwareMonitor", "LibreHardwareMonitor"), (r"root\OpenHardwareMonitor", "OpenHardwareMonitor")];

#[derive(Default)]
pub struct Lhm {
    svc: Option<(IWbemServices, &'static str)>,
    next_try: Option<Instant>,
    hw: HashMap<String, String>,
    hw_at: Option<Instant>,
    cache: Vec<SensorReading>,
    last: Option<Instant>,
    interval: Duration,
}

struct Var(VARIANT);

impl Drop for Var {
    fn drop(&mut self) {
        unsafe { _ = VariantClear(&mut self.0) };
    }
}

impl Var {
    fn str(&self) -> String {
        unsafe {
            let v = &self.0.Anonymous.Anonymous;
            if v.vt == VT_BSTR { v.Anonymous.bstrVal.to_string() } else { String::new() }
        }
    }

    fn f32(&self) -> Option<f32> {
        unsafe {
            let v = &self.0.Anonymous.Anonymous;
            match v.vt {
                VT_R4 => Some(v.Anonymous.fltVal),
                VT_R8 => Some(v.Anonymous.dblVal as f32),
                _ => None,
            }
        }
    }
}

fn connect() -> Option<(IWbemServices, &'static str)> {
    unsafe {
        let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
        NAMESPACES.iter().find_map(|&(ns, src)| {
            let e = BSTR::new();
            let svc =
                loc.ConnectServer(&BSTR::from(ns), &e, &e, &e, WBEM_FLAG_CONNECT_USE_MAX_WAIT.0, &e, None).ok()?;
            // RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE
            CoSetProxyBlanket(
                &svc,
                10,
                0,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
            .ok()?;
            // Namespace may linger after the app exits; require at least one sensor.
            let mut any = false;
            query(&svc, "SELECT Name FROM Sensor", &[w!("Name")], |_| any = true).ok()?;
            any.then_some((svc, src))
        })
    }
}

fn query<const N: usize>(
    svc: &IWbemServices,
    wql: &str,
    props: &[PCWSTR; N],
    mut f: impl FnMut(&[Var; N]),
) -> Result<()> {
    unsafe {
        let e = svc.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from(wql),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )?;
        loop {
            let mut objs: [Option<IWbemClassObject>; 64] = std::array::from_fn(|_| None);
            let mut n = 0;
            e.Next(5000, &mut objs, &mut n).ok()?;
            for o in objs[..n as usize].iter().flatten() {
                let vals = props.map(|p| {
                    let mut v = Var(VARIANT::default());
                    _ = o.Get(p, 0, &mut v.0, None, None);
                    v
                });
                f(&vals);
            }
            if (n as usize) < objs.len() {
                return Ok(());
            }
        }
    }
}

fn kind(t: &str) -> Option<SensorKind> {
    Some(match t {
        "Temperature" => SensorKind::Temperature,
        "Fan" => SensorKind::Fan,
        "Power" => SensorKind::Power,
        "Voltage" => SensorKind::Voltage,
        "Clock" => SensorKind::Clock,
        "Load" | "Control" => SensorKind::Load,
        "Current" => SensorKind::Other,
        _ => return None,
    })
}

impl Lhm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&mut self) -> Vec<SensorReading> {
        let now = Instant::now();
        if self.svc.is_none() {
            if self.next_try.is_some_and(|t| now < t) {
                return Vec::new();
            }
            self.svc = connect();
            if self.svc.is_none() {
                self.next_try = Some(now + RECONNECT);
                return Vec::new();
            }
            self.hw_at = None;
            self.last = None;
        }
        if self.last.is_some_and(|t| t.elapsed() < self.interval) {
            return self.cache.clone();
        }
        let Some((svc, src)) = self.svc.clone() else { return Vec::new() };
        if self.hw_at.is_none_or(|t| t.elapsed() >= HW_REFRESH) {
            self.hw.clear();
            _ = query(&svc, "SELECT Identifier, Name FROM Hardware", &[w!("Identifier"), w!("Name")], |[id, name]| {
                self.hw.insert(id.str(), name.str());
            });
            self.hw_at = Some(now);
        }
        let mut out = Vec::new();
        let r = query(
            &svc,
            "SELECT Name, SensorType, Value, Parent FROM Sensor",
            &[w!("Name"), w!("SensorType"), w!("Value"), w!("Parent")],
            |[name, ty, value, parent]| {
                if let (Some(k), Some(v)) = (kind(&ty.str()), value.f32().filter(|v| v.is_finite())) {
                    let parent = parent.str();
                    let hw = self.hw.get(&parent).map_or(parent.as_str(), String::as_str);
                    out.push(reading(src, hw, &name.str(), k, v));
                }
            },
        );
        if r.is_err() || out.is_empty() {
            // App closed (RPC failure) or provider gone.
            self.svc = None;
            self.cache.clear();
            self.next_try = Some(Instant::now() + RECONNECT);
            return Vec::new();
        }
        self.interval = if now.elapsed() > SLOW_QUERY { SLOW_INTERVAL } else { Duration::ZERO };
        self.last = Some(Instant::now());
        self.cache = out;
        self.cache.clone()
    }
}
