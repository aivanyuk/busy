use crate::util::Every;
use busy_core::{BatteryInfo, Module, Snapshot, Source};
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Foundation::{GENERIC_READ, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Power::*;
use windows::core::{Owned, PCWSTR};

#[derive(Default)]
pub struct Battery {
    devs: Vec<Dev>,
    refresh: Every,
    design: Option<u32>,
    full: Option<u32>,
    cycles: Option<u32>,
}

struct Dev {
    h: Owned<HANDLE>,
    tag: u32,
}

impl Source for Battery {
    fn module(&self) -> Module {
        Module::Battery
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        let mut ps = SYSTEM_POWER_STATUS::default();
        if unsafe { GetSystemPowerStatus(&mut ps) }.is_err() {
            return;
        }
        // 128 = no system battery, 255 = unknown.
        if ps.BatteryFlag & 128 != 0 {
            self.devs.clear();
            snap.battery = None;
            return;
        }
        if self.refresh.due(60) {
            self.refresh_info();
        }
        let mut charging = ps.BatteryFlag & 8 != 0 && ps.BatteryFlag != 255;
        let mut rate = None;
        let mut failed = false;
        for d in &self.devs {
            match d.status() {
                Some(st) => {
                    charging |= st.PowerState & BATTERY_CHARGING != 0;
                    if st.Rate as u32 != BATTERY_UNKNOWN_RATE {
                        *rate.get_or_insert(0) += st.Rate;
                    }
                }
                None => failed = true,
            }
        }
        if failed {
            // Tag changes when a battery is swapped; re-enumerate next sample.
            self.refresh = Every::default();
        }
        snap.battery = Some(BatteryInfo {
            percent: if ps.BatteryLifePercent <= 100 { ps.BatteryLifePercent as f32 } else { 0.0 },
            charging,
            ac_online: ps.ACLineStatus == 1,
            secs_remaining: (ps.BatteryLifeTime != u32::MAX).then_some(ps.BatteryLifeTime),
            rate_mw: rate,
            full_capacity_mwh: self.full,
            design_capacity_mwh: self.design,
            cycle_count: self.cycles,
        });
    }
}

impl Battery {
    fn refresh_info(&mut self) {
        self.devs = enumerate();
        let (mut design, mut full, mut cycles) = (None, None, None);
        let add = |acc: &mut Option<u32>, v: u32| {
            if v != 0 && v != BATTERY_UNKNOWN_CAPACITY {
                *acc = Some(acc.unwrap_or(0) + v);
            }
        };
        for bi in self.devs.iter().filter_map(Dev::info) {
            // Relative-capacity batteries report unitless values, not mWh.
            if bi.Capabilities & BATTERY_CAPACITY_RELATIVE == 0 {
                add(&mut design, bi.DesignedCapacity);
                add(&mut full, bi.FullChargedCapacity);
            }
            add(&mut cycles, bi.CycleCount);
        }
        (self.design, self.full, self.cycles) = (design, full, cycles);
    }
}

impl Dev {
    fn ioctl<I, O: Default>(&self, code: u32, input: &I) -> Option<O> {
        let mut out = O::default();
        let mut ret = 0u32;
        unsafe {
            DeviceIoControl(
                *self.h,
                code,
                Some((input as *const I).cast()),
                size_of::<I>() as u32,
                Some((&mut out as *mut O).cast()),
                size_of::<O>() as u32,
                Some(&mut ret),
                None,
            )
        }
        .ok()?;
        (ret as usize == size_of::<O>()).then_some(out)
    }

    fn info(&self) -> Option<BATTERY_INFORMATION> {
        let q = BATTERY_QUERY_INFORMATION { BatteryTag: self.tag, InformationLevel: BatteryInformation, AtRate: 0 };
        self.ioctl(IOCTL_BATTERY_QUERY_INFORMATION, &q)
    }

    fn status(&self) -> Option<BATTERY_STATUS> {
        let q = BATTERY_WAIT_STATUS { BatteryTag: self.tag, ..Default::default() };
        self.ioctl(IOCTL_BATTERY_QUERY_STATUS, &q)
    }
}

fn enumerate() -> Vec<Dev> {
    let guid = GUID_DEVCLASS_BATTERY;
    let Ok(set) =
        (unsafe { SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) })
    else {
        return Vec::new();
    };
    let mut devs = Vec::new();
    for i in 0..8 {
        let mut did =
            SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
        if unsafe { SetupDiEnumDeviceInterfaces(set, None, &guid, i, &mut did) }.is_err() {
            break;
        }
        if let Some(d) = open(set, &did) {
            devs.push(d);
        }
    }
    let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };
    devs
}

fn open(set: HDEVINFO, did: &SP_DEVICE_INTERFACE_DATA) -> Option<Dev> {
    let mut need = 0u32;
    let _ = unsafe { SetupDiGetDeviceInterfaceDetailW(set, did, None, 0, Some(&mut need), None) };
    if (need as usize) < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
        return None;
    }
    let mut buf = vec![0u64; (need as usize).div_ceil(8)];
    let detail = buf.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    // cbSize is the fixed header size, not the buffer size.
    unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    unsafe { SetupDiGetDeviceInterfaceDetailW(set, did, Some(detail), need, None, None) }.ok()?;
    let path = PCWSTR(unsafe { std::ptr::addr_of!((*detail).DevicePath) }.cast());
    // The QUERY_TAG/INFORMATION/STATUS IOCTL codes encode FILE_READ_ACCESS, so read access is all the I/O manager
    // checks; only IOCTL_BATTERY_SET_INFORMATION needs write, and we never send it.
    // SAFETY: `path` points into `buf`, NUL-terminated by SetupDiGetDeviceInterfaceDetailW, and outlives the call.
    let h = unsafe {
        CreateFileW(
            path,
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()?;
    let mut d = Dev { h: unsafe { Owned::new(h) }, tag: 0 };
    d.tag = d.ioctl::<u32, u32>(IOCTL_BATTERY_QUERY_TAG, &0)?;
    (d.tag != BATTERY_TAG_INVALID).then_some(d)
}
