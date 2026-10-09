//! Physical GPU inventory is separate from the logical DirectML adapter IDs.
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    // Representative DXGI index, not the physical GPU's ordinal.
    pub id: i32,
    pub name: String,
    pub memory_bytes: Option<u64>,
    pub physical_id: String,
    pub adapter_ids: Vec<i32>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuAdapter {
    pub id: i32,
    pub name: String,
    pub memory_bytes: u64,
    pub physical_ids: Vec<String>,
}
#[derive(Default)]
pub struct Inventory {
    pub devices: Vec<GpuDevice>,
    pub adapters: Vec<GpuAdapter>,
    pub physical_detection_complete: bool,
}
struct PhysicalIdentity {
    key: String,
    memory_bytes: Option<u64>,
}
struct Observation {
    id: i32,
    name: String,
    memory_bytes: u64,
    physical: Vec<PhysicalIdentity>,
    identification_complete: bool,
}
fn physical_id(key: &str) -> Option<String> {
    let key = key
        .trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_uppercase();
    if key.is_empty() {
        return None;
    }
    // Expose only a fingerprint, not the registry/device-instance path.
    Some(blake3::hash(key.as_bytes()).to_hex().to_string())
}
fn group(observations: Vec<Observation>) -> Inventory {
    let mut devices: BTreeMap<String, GpuDevice> = BTreeMap::new();
    let mut adapters = Vec::new();
    let mut complete = true;
    for observation in observations {
        complete &= observation.identification_complete && !observation.physical.is_empty();
        let mut physical_ids = Vec::new();
        for physical in observation.physical {
            let Some(identity) = physical_id(&physical.key) else {
                complete = false;
                continue;
            };
            if !physical_ids.contains(&identity) {
                physical_ids.push(identity.clone());
            }
            let device = devices
                .entry(identity.clone())
                .or_insert_with(|| GpuDevice {
                    id: observation.id,
                    name: observation.name.clone(),
                    memory_bytes: physical.memory_bytes,
                    physical_id: identity,
                    adapter_ids: Vec::new(),
                });
            if !device.adapter_ids.contains(&observation.id) {
                device.adapter_ids.push(observation.id);
            }
            if observation.id < device.id {
                device.id = observation.id;
                device.name = observation.name.clone();
            }
            // Aliases share memory. Never sum their reported VRAM.
            device.memory_bytes = match (device.memory_bytes, physical.memory_bytes) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
        adapters.push(GpuAdapter {
            id: observation.id,
            name: observation.name,
            memory_bytes: observation.memory_bytes,
            physical_ids,
        });
    }
    let mut devices = devices.into_values().collect::<Vec<_>>();
    for device in &mut devices {
        device.adapter_ids.sort_unstable();
    }
    devices.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.physical_id.cmp(&b.physical_id))
    });
    adapters.sort_by_key(|adapter| adapter.id);
    Inventory {
        devices,
        adapters,
        physical_detection_complete: complete,
    }
}

#[cfg(target_os = "windows")]
mod windows_inventory {
    use super::*;
    use windows::{
        Wdk::Graphics::Direct3D::{
            D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMT_PHYSICAL_ADAPTER_COUNT,
            D3DKMT_PNP_KEY_HARDWARE, D3DKMT_QUERY_PHYSICAL_ADAPTER_PNP_KEY,
            D3DKMT_QUERYADAPTERINFO, D3DKMT_SEGMENTGROUPSIZEINFO, D3DKMTCloseAdapter,
            D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, KMTQAITYPE_GETSEGMENTGROUPSIZE,
            KMTQAITYPE_PHYSICALADAPTERCOUNT, KMTQAITYPE_PHYSICALADAPTERPNPKEY,
            KMTQUERYADAPTERINFOTYPE,
        },
        Win32::{
            Foundation::LUID,
            Graphics::{
                Direct3D::D3D_FEATURE_LEVEL_11_0,
                Direct3D12::{D3D12CreateDevice, ID3D12Device},
                Dxgi::{
                    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
                    IDXGIFactory1,
                },
            },
        },
        core::PWSTR,
    };
    struct KernelAdapter(u32);
    impl Drop for KernelAdapter {
        fn drop(&mut self) {
            // Kernel handles are not Win32 HANDLEs; CloseHandle would be incorrect.
            unsafe {
                let _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: self.0 });
            }
        }
    }
    fn query<T>(adapter: &KernelAdapter, kind: KMTQUERYADAPTERINFOTYPE, data: &mut T) -> bool {
        let mut query = D3DKMT_QUERYADAPTERINFO {
            hAdapter: adapter.0,
            Type: kind,
            pPrivateDriverData: (data as *mut T).cast(),
            PrivateDriverDataSize: std::mem::size_of::<T>() as u32,
        };
        // The typed buffers remain alive for the entire synchronous driver call.
        unsafe { D3DKMTQueryAdapterInfo(&mut query).0 >= 0 }
    }
    fn identify(luid: LUID, reported_memory: u64) -> (Vec<PhysicalIdentity>, bool) {
        let mut opened = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: luid,
            hAdapter: 0,
        };
        if unsafe { D3DKMTOpenAdapterFromLuid(&mut opened).0 < 0 } {
            return (Vec::new(), false);
        }
        let adapter = KernelAdapter(opened.hAdapter);
        let mut count = D3DKMT_PHYSICAL_ADAPTER_COUNT::default();
        if !query(&adapter, KMTQAITYPE_PHYSICALADAPTERCOUNT, &mut count)
            || !(1..=64).contains(&count.Count)
        {
            return (Vec::new(), false);
        }
        let mut identities = Vec::new();
        for index in 0..count.Count {
            let mut buffer = [0u16; 4096];
            let mut capacity = buffer.len() as u32;
            let mut key = D3DKMT_QUERY_PHYSICAL_ADAPTER_PNP_KEY {
                PhysicalAdapterIndex: index,
                PnPKeyType: D3DKMT_PNP_KEY_HARDWARE,
                pDest: PWSTR(buffer.as_mut_ptr()),
                pCchDest: &mut capacity,
            };
            if !query(&adapter, KMTQAITYPE_PHYSICALADAPTERPNPKEY, &mut key)
                || capacity as usize > buffer.len()
            {
                continue;
            }
            let Some(end) = buffer.iter().position(|value| *value == 0) else {
                continue;
            };
            if end == 0 {
                continue;
            }
            let memory_bytes = if count.Count == 1 {
                Some(reported_memory)
            } else {
                let mut sizes = D3DKMT_SEGMENTGROUPSIZEINFO {
                    PhysicalAdapterIndex: index,
                    ..Default::default()
                };
                query(&adapter, KMTQAITYPE_GETSEGMENTGROUPSIZE, &mut sizes)
                    .then_some(sizes.LegacyInfo.DedicatedVideoMemorySize)
            };
            let Ok(key) = String::from_utf16(&buffer[..end]) else {
                continue;
            };
            identities.push(PhysicalIdentity { key, memory_bytes });
        }
        let complete = identities.len() == count.Count as usize;
        (identities, complete)
    }
    pub fn discover() -> Inventory {
        let mut observations = Vec::new();
        let mut enumeration_complete = true;
        let mut enumeration_finished = false;
        unsafe {
            let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
                return Inventory::default();
            };
            for id in 0..128 {
                let adapter = match factory.EnumAdapters1(id) {
                    Ok(adapter) => adapter,
                    Err(error) => {
                        enumeration_complete &= error.code() == DXGI_ERROR_NOT_FOUND;
                        enumeration_finished = true;
                        break;
                    }
                };
                let Ok(desc) = adapter.GetDesc1() else {
                    enumeration_complete = false;
                    continue;
                };
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let mut device: Option<ID3D12Device> = None;
                if D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device).is_err() {
                    continue;
                }
                let end = desc
                    .Description
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(desc.Description.len());
                let memory_bytes = desc.DedicatedVideoMemory as u64;
                let (physical, identification_complete) = identify(desc.AdapterLuid, memory_bytes);
                observations.push(Observation {
                    id: id as i32,
                    name: String::from_utf16_lossy(&desc.Description[..end]),
                    memory_bytes,
                    physical,
                    identification_complete,
                });
            }
        }
        let mut inventory = group(observations);
        inventory.physical_detection_complete &= enumeration_complete && enumeration_finished;
        inventory
    }
}
#[cfg(target_os = "windows")]
pub fn discover() -> Inventory {
    windows_inventory::discover()
}
#[cfg(not(target_os = "windows"))]
pub fn discover() -> Inventory {
    Inventory::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed(id: i32, key: Option<&str>) -> Observation {
        Observation {
            id,
            name: "NVIDIA GeForce GTX 1650".into(),
            memory_bytes: 4 * 1024 * 1024 * 1024,
            physical: key
                .into_iter()
                .map(|key| PhysicalIdentity {
                    key: key.into(),
                    memory_bytes: Some(4 * 1024 * 1024 * 1024),
                })
                .collect(),
            identification_complete: key.is_some(),
        }
    }
    #[test]
    fn logical_aliases_of_one_physical_gpu_are_grouped_without_summing_vram() {
        let inventory = group(vec![
            observed(1, Some("PCI\\GPU-A")),
            observed(0, Some("pci/gpu-a\\")),
        ]);
        assert_eq!(inventory.devices.len(), 1);
        assert_eq!(inventory.devices[0].adapter_ids, vec![0, 1]);
        assert_eq!(inventory.devices[0].id, 0);
        assert_eq!(
            inventory.devices[0].memory_bytes,
            Some(4 * 1024 * 1024 * 1024)
        );
        assert_eq!(inventory.adapters.len(), 2);
        assert!(inventory.physical_detection_complete);
    }
    #[test]
    fn identical_gpu_models_at_different_physical_locations_remain_separate() {
        let inventory = group(vec![
            observed(0, Some("PCI\\GPU-A")),
            observed(1, Some("PCI\\GPU-B")),
        ]);
        assert_eq!(inventory.devices.len(), 2);
        assert_eq!(inventory.devices[0].name, inventory.devices[1].name);
        assert_ne!(
            inventory.devices[0].physical_id,
            inventory.devices[1].physical_id
        );
    }
    #[test]
    fn unidentified_logical_adapters_are_not_counted_as_physical_gpus() {
        let inventory = group(vec![observed(0, Some("PCI\\GPU-A")), observed(1, None)]);
        assert_eq!(inventory.devices.len(), 1);
        assert_eq!(inventory.adapters.len(), 2);
        assert!(!inventory.physical_detection_complete);
    }
    #[test]
    fn linked_adapter_preserves_multiple_physical_devices_without_claiming_aggregate_vram() {
        let mut adapter = observed(3, Some("PCI\\GPU-A"));
        adapter.physical.push(PhysicalIdentity {
            key: "PCI\\GPU-B".into(),
            memory_bytes: None,
        });
        let inventory = group(vec![adapter]);
        assert_eq!(inventory.devices.len(), 2);
        assert_eq!(inventory.adapters.len(), 1);
        assert!(
            inventory
                .devices
                .iter()
                .all(|device| device.adapter_ids == vec![3])
        );
        assert!(
            inventory
                .devices
                .iter()
                .any(|device| device.memory_bytes.is_none())
        );
    }
    #[test]
    fn empty_identity_does_not_create_a_physical_device() {
        let inventory = group(vec![observed(0, Some("  "))]);
        assert!(inventory.devices.is_empty());
        assert_eq!(inventory.adapters.len(), 1);
        assert!(!inventory.physical_detection_complete);
    }
    #[test]
    fn windows_device_identity_mapping_is_reported() {
        let inventory = discover();
        println!(
            "physical GPUs: {:?}; logical adapters: {:?}; complete: {}",
            inventory.devices, inventory.adapters, inventory.physical_detection_complete
        );
        for device in &inventory.devices {
            assert!(!device.adapter_ids.is_empty());
            assert!(device.adapter_ids.iter().all(|id| {
                inventory.adapters.iter().any(|adapter| {
                    adapter.id == *id && adapter.physical_ids.contains(&device.physical_id)
                })
            }));
        }
    }
}
