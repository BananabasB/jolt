"use client";

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ModeToggle } from "@/components/ui/mode-toggle";
import { Zap, Usb, AlertCircle, CheckCircle, Loader2, Syringe, LoaderPinwheel, FolderSearch } from "lucide-react";
import { ButtonGroup } from "@/components/ui/button-group";
import { FetchPayloads } from "@/components/fetch-payloads";
interface DeviceInfo {
  vendor_id: number;
  product_id: number;
  manufacturer?: string;
  product?: string;
  serial_number?: string;
}

interface RcmStatus {
  device_connected: boolean;
  device_info?: DeviceInfo;
  rcm_detected: boolean;
}

export default function Home() {
  const [rcmStatus, setRcmStatus] = useState<RcmStatus | null>(null);
  const [isManuallyScanning, setIsManuallyScanning] = useState(false);
  const [selectedPayload, setSelectedPayload] = useState<string>("");
  const [usbDevices, setUsbDevices] = useState<DeviceInfo[]>([]);
  const [showDevices, setShowDevices] = useState(false);
  const [isInjecting, setIsInjecting] = useState(false);
  const [version, setVersion] = useState<string>("");
  useEffect(() => {
    // this function will only run after the component mounts on the client
    const fetchVersion = async () => {
      try {
        const appVersion: string = await invoke("get_app_version");
        setVersion(appVersion);
      } catch (error) {
        console.error("failed to fetch app version:", error);
        setVersion("error");
      }
    };

    fetchVersion();
    // the empty array [] ensures this runs only once after initial render
  }, []);

  const scanForDevice = async () => {
    try {
      const status: RcmStatus = await invoke("get_rcm_status");
      setRcmStatus(status);
    } catch (error) {
      console.error("Failed to scan for device:", error);
    }
  };

  const manualScanForDevice = async () => {
    setIsManuallyScanning(true);
    await scanForDevice();
    setIsManuallyScanning(false);
  };

  const injectPayload = async () => {
    if (!selectedPayload || !rcmStatus?.rcm_detected) {
      return;
    }

    if (isInjecting) {
      alert("Injection already in progress");
      return;
    }

    setIsInjecting(true);
    try {
      const result: string = await invoke("inject_payload", { payloadPath: selectedPayload });
      alert(`Success: ${result}`);
    } catch (error) {
      alert(`Injection failed: ${error}`);
    } finally {
      setIsInjecting(false);
    }
  };

  useEffect(() => {
    scanForDevice();
    // Auto-scan every 2 seconds, but not during injection
    const interval = setInterval(() => {
      if (!isInjecting) {
        scanForDevice();
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [isInjecting]);

  return (
    <main className="flex min-h-screen bg-background items-center justify-center relative p-4">
      <div className="absolute top-4 right-4">
        <ModeToggle />
      </div>

      <div className="w-full max-w-2xl space-y-6">
        {/* Header */}
        <div className="text-center">
          <div className="flex items-center justify-center gap-2 mb-2">
            <Zap size={24} />
            <h1 className="text-2xl font-bold">jolt</h1>
          </div>
          <p className="text-muted-foreground">Nintendo Switch RCM tool</p>
        </div>

        {/* Device Status */}
        <div className="border rounded-lg p-4 space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold flex items-center gap-2">
              <Usb size={20} />
              device status
            </h2>
            <div className="flex gap-2">
              <Button
                onClick={manualScanForDevice}
                disabled={isManuallyScanning}
                size="sm"
                variant="outline"
              >
                {isManuallyScanning ? (
                  <Loader2 size={16} className="animate-spin" />
                ) : (
                  "scan for RCM"
                )}
              </Button>
              <Button
                onClick={() => setShowDevices(!showDevices)}
                size="sm"
                variant="outline"
              >
                {showDevices ? "hide" : "show"} all USB
              </Button>
            </div>
          </div>

          <div className="space-y-2">
            <div className="flex items-center gap-2">
              {rcmStatus?.rcm_detected ? (
                <CheckCircle size={20} className="text-green-500" />
              ) : (
                <AlertCircle size={20} className="text-red-500" />
              )}
              <span>
                {rcmStatus?.rcm_detected
                  ? "Switch in RCM mode detected"
                  : "no Switch in RCM mode found"}
              </span>
            </div>

            {rcmStatus?.device_info && (
              <div className="text-sm text-muted-foreground ml-6">
                <div>vendor ID: 0x{rcmStatus.device_info.vendor_id.toString(16).toUpperCase()}</div>
                <div>product ID: 0x{rcmStatus.device_info.product_id.toString(16).toUpperCase()}</div>
                {rcmStatus.device_info.manufacturer && (
                  <div>manufacturer: {rcmStatus.device_info.manufacturer}</div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* USB Devices List */}
        {showDevices && (
          <div className="border rounded-lg p-4 space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold">USB devices ({usbDevices.length})</h2>
            </div>

            <div className="space-y-2 max-h-64 overflow-y-auto">
              {usbDevices.map((device, index) => (
                <div key={index} className="border rounded p-2 text-sm">
                  <div className="font-mono">
                    vendor: 0x{device.vendor_id.toString(16).toUpperCase().padStart(4, '0')} |
                    product: 0x{device.product_id.toString(16).toUpperCase().padStart(4, '0')}
                  </div>
                  {device.manufacturer && <div>Manufacturer: {device.manufacturer}</div>}
                  {device.product && <div>Product: {device.product}</div>}
                  {device.serial_number && <div>Serial: {device.serial_number}</div>}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Payload Section */}
        <div className="border rounded-lg p-4 space-y-3">
          <h2 className="text-lg font-semibold">payload</h2>

          <div className="space-y-3">
            <ButtonGroup className="w-full">
              <Input
                type="text"
                placeholder="select payload file..."
                value={selectedPayload}
                onChange={(e) => setSelectedPayload(e.target.value)}
                readOnly
                className="flex-1"
              />
              <Button
                variant="outline"
                onClick={async () => {
                  try {
                    const filePath = await open({
                      multiple: false,
                      filters: [{
                        name: "Payload files",
                        extensions: ["bin", "payload"]
                      }]
                    });
                    if (filePath) {
                      setSelectedPayload(filePath as string);
                    }
                  } catch (error) {
                    console.error("File picker error:", error);
                  }
                }}
              >
                <FolderSearch /> browse
              </Button>
                <FetchPayloads onSelectPayload={setSelectedPayload} />
            </ButtonGroup>

            <Button
              onClick={injectPayload}
              disabled={!rcmStatus?.rcm_detected || !selectedPayload || isInjecting}
              className="w-full"
            >
              {isInjecting ? (
                <LoaderPinwheel size={16} className="animate-spin" />
              ) : (
                <>
                  <Syringe size={16} className="mr-2" />
                  inject payload
                </>
              )}
            </Button>
          </div>
        </div>
      </div>
      <div className="absolute text-muted-foreground text-sm bottom-3 right-3">v{version}</div>
    </main>
  );
}
