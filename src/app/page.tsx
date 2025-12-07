"use client";

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ModeToggle } from "@/components/ui/mode-toggle";
import { Zap, Usb, AlertCircle, CheckCircle, Loader2, Syringe, LoaderPinwheel, FolderSearch, CircleX, Undo2, Globe, Lock, Unlock } from "lucide-react";
import { ButtonGroup } from "@/components/ui/button-group";
import { FetchPayloads } from "@/components/fetch-payloads";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";

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
  switch_connected_not_rcm: boolean;
}

export default function Home() {
  const [rcmStatus, setRcmStatus] = useState<RcmStatus | null>(null);
  const [isManuallyScanning, setIsManuallyScanning] = useState(false);
  const [selectedPayload, setSelectedPayload] = useState<string>("");
  const [usbDevices, setUsbDevices] = useState<DeviceInfo[]>([]);
  const [showDevices, setShowDevices] = useState(false);
  const [isInjecting, setIsInjecting] = useState(false);
  const [version, setVersion] = useState<string>("");

  // State for external links history and current index
  const [externalHistory, setExternalHistory] = useState<string[]>([]);
  const [currentExternalIndex, setCurrentExternalIndex] = useState<number>(-1);

  useEffect(() => {
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
  }, []);

  // Wrap Link component to track external link clicks
  const ExternalLink: React.FC<{ href: string; children: React.ReactNode }> = ({ href, children }) => {
    const handleClick = (e: React.MouseEvent) => {
      // Only track if href is external (starts with http or https)
      if (/^https?:\/\//.test(href)) {
        e.preventDefault();
        let newHistory = externalHistory.slice(0, currentExternalIndex + 1);
        newHistory.push(href);
        setExternalHistory(newHistory);
        setCurrentExternalIndex(newHistory.length - 1);
        window.open(href, "_blank", "noopener,noreferrer");
      }
    };
    return (
      <a href={href} onClick={handleClick} target="_blank" rel="noopener noreferrer" className="underline">
        {children}
      </a>
    );
  };

  const scanForDevice = async () => {
    try {
      const status: RcmStatus = await invoke("get_rcm_status");
      setRcmStatus(status);
    } catch (error) {
      console.error("Failed to scan for device:", error);
    }
  };

  const listUsbDevices = async () => {
    try {
      const devices: DeviceInfo[] = await invoke("list_usb_devices");
      setUsbDevices(devices);
    } catch (error) {
      console.error("failed to list usb devices:", error);
      setUsbDevices([]);
    }
  };

  const manualScanForDevice = async () => {
    setIsManuallyScanning(true);
    await scanForDevice();
    await listUsbDevices();
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
    listUsbDevices();
    
    const interval = setInterval(() => {
      if (!isInjecting) {
        scanForDevice();
        if (showDevices) {
          listUsbDevices();
        }
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [isInjecting, showDevices]);

  const goBackExternal = () => {
    // Close the mini-browser and return to main app view
    setCurrentExternalIndex(-1);
  };

  const openInBrowser = async () => {
    if (currentExternalIndex >= 0 && externalHistory[currentExternalIndex]) {
      try {
        await invoke("open_url", { url: externalHistory[currentExternalIndex] });
      } catch (error) {
        console.error("Failed to open in system browser:", error);
      }
    }
  };

  // Helper to parse URL into protocol, domain, and path
  const parseUrl = (url: string) => {
    try {
      const u = new URL(url);
      return {
        protocol: u.protocol,
        domain: u.host,
        path: u.pathname + u.search + u.hash,
      };
    } catch {
      return {
        protocol: "",
        domain: url,
        path: "",
      };
    }
  };

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
              ) : rcmStatus?.switch_connected_not_rcm ? (
                <AlertCircle size={20} className="text-orange-500" />
              ) : (
                <CircleX size={20} className="text-red-500" />
              )}
              <div className="flex flex-col gap-1">
                {rcmStatus?.rcm_detected
                  ? "Switch in RCM mode detected"
                  : rcmStatus?.switch_connected_not_rcm
                    ? (
                        <>
                          <span>Switch detected but not in RCM mode</span>
                          <span className="text-sm text-muted-foreground">
                            Please reboot your Nintendo Switch into RCM mode. <ExternalLink href="https://switch.hacks.guide/user_guide/rcm/entering_rcm.html">Show me how</ExternalLink>
                          </span>
                        </>
                      )
                    : "no Switch found"}
              </div>
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
              {usbDevices.length === 0 ? (
                <div className="text-sm text-muted-foreground">no USB devices found</div>
              ) : (
                usbDevices.map((device, index) => (
                  <div key={index} className="border rounded p-2 text-sm">
                    <div className="font-mono">
                      vendor: 0x{device.vendor_id.toString(16).toUpperCase().padStart(4, '0')} |
                      product: 0x{device.product_id.toString(16).toUpperCase().padStart(4, '0')}
                    </div>
                    {device.manufacturer && <div>Manufacturer: {device.manufacturer}</div>}
                    {device.product && <div>Product: {device.product}</div>}
                    {device.serial_number && <div>Serial: {device.serial_number}</div>}
                  </div>
                ))
              )}
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
                    const filePath = await openDialog({
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

      {/* External link mini-bar with iframe */}
      {currentExternalIndex >= 0 && (
        
        <div className="fixed inset-0 z-50">
          <iframe
            src={externalHistory[currentExternalIndex]}
            title="External Content"
            className="w-full h-full border-none"
            sandbox="allow-scripts allow-same-origin allow-popups allow-forms"
          />
          <div className="z-50 transition bg-transparent absolute [--radius:9999px] bottom-2 left-1/2 transform -translate-x-1/2 flex min-w-[200px] max-w-2xl overflow-hidden items-end h-30 pb-2 px-2 gap-2">
            <InputGroup className="aspect-square bg-card! h-10 flex-none w-10">
              <InputGroupButton size={"icon-sm"} onClick={goBackExternal} className="cursor-pointer select-none rounded-full w-10 h-10 flex items-center justify-center pb-0">
                <Undo2 />
              </InputGroupButton>
            </InputGroup>
            <InputGroup className="grow bg-card! flex h-10 items-center gap-2 relative overflow-hidden">
              <InputGroupAddon>
                {(() => {
                  const { protocol } = parseUrl(externalHistory[currentExternalIndex]);
                  if (protocol === "https:") {
                    // shadcn/ui Popover for secure connection
                    return (
                      <Popover>
                        <PopoverTrigger asChild>
                          <InputGroupButton
                            size="icon-xs"
                            className="cursor-pointer rounded-full transition"
                            aria-label="Secure Connection"
                          >
                            <Lock className="w-4 h-4" />
                          </InputGroupButton>
                        </PopoverTrigger>
                        <PopoverContent
                          align="start"
                          className="flex flex-col gap-1 rounded-xl text-sm min-w-[180px] border-green-500/60 bg-card shadow-lg"
                        >
                          <div className="flex items-center gap-2 mb-1">
                            <Lock className="w-4 h-4 text-green-500" />
                            <span className="font-semibold text-green-700 dark:text-green-300">Secure connection</span>
                          </div>
                          <span className="text-muted-foreground">Your connection is encrypted and secure.</span>
                        </PopoverContent>
                      </Popover>
                    );
                  } else {
                    // shadcn/ui Popover for unsecure connection
                    return (
                      <Popover>
                        <PopoverTrigger asChild>
                          <InputGroupButton
                            size="icon-xs"
                            className="cursor-pointer rounded-full border transition"
                            aria-label="Unsecure Connection"
                          >
                            <Unlock className="w-4 h-4" />
                          </InputGroupButton>
                        </PopoverTrigger>
                        <PopoverContent
                          align="start"
                          className="flex flex-col gap-1 rounded-xl text-sm min-w-[180px] border-red-500/60 bg-card shadow-lg"
                        >
                          <div className="flex items-center gap-2 mb-1">
                            <Unlock className="w-4 h-4 text-red-500" />
                            <span className="font-semibold text-red-700 dark:text-red-300">Unsecure connection</span>
                          </div>
                          <span className="text-muted-foreground">Your connection is not encrypted. Be cautious.</span>
                        </PopoverContent>
                      </Popover>
                    );
                  }
                })()}
              </InputGroupAddon>
              <InputGroupInput
                readOnly
                className="max-w-full text-center"
                value={(() => {
                  const { domain } = parseUrl(externalHistory[currentExternalIndex]);
                  return domain;
                })()}
                aria-label="Current URL"
              />
              <InputGroupAddon align="inline-end">
              <InputGroupButton onClick={openInBrowser} size="icon-xs" className="flex-none">
                <Globe />
              </InputGroupButton>
            </InputGroupAddon>
            </InputGroup>
            
          </div>
        </div>
      )}

      <div className="absolute text-muted-foreground text-sm bottom-3 right-3">v{version}</div>
    </main>
  );
}