'use client';
import { DownloadCloud, Check } from "lucide-react";
import { Button } from "./ui/button";
import { useState, useEffect } from 'react';
import { Octokit } from "octokit";
import { invoke } from "@tauri-apps/api/core";
import { exists } from "@tauri-apps/plugin-fs";
import { downloadDir, join } from "@tauri-apps/api/path";
import { Store } from "@tauri-apps/plugin-store";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "./ui/dialog";
import {
    Card,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "./ui/card";
import { Badge } from './ui/badge';

const octokit = new Octokit();
let store: Store | null = null;

// GitHub API types
interface GitHubAsset {
    id: number;
    name: string;
    browser_download_url: string;
    size: number;
}

interface GitHubRelease {
    id: number;
    name: string | null;
    tag_name: string;
    published_at: string;
    assets: GitHubAsset[];
}

export async function getReleases(repo: { owner: string, repo: string }): Promise<GitHubRelease[]> {
    try {
        const releases = await octokit.rest.repos.listReleases({
            owner: repo.owner,
            repo: repo.repo,
        });
        return releases.data as GitHubRelease[];
    } catch (error) {
        console.error('oh that\'s a bit not allowed:', error);
        return [];
    }
}

async function downloadPayload(assetUrl: string, fileName: string) {
    try {
        // Use Tauri backend to download and save the file
        const filePath: string = await invoke("download_payload", {
            url: assetUrl,
            filename: fileName
        });
        console.log(`Downloaded ${fileName} to ${filePath}`);
        return filePath;
    } catch (error) {
        console.error("Download failed:", error);
        throw error;
    }
}

interface FetchPayloadsProps {
    onSelectPayload?: (path: string) => void;
}

export function FetchPayloads({ onSelectPayload }: FetchPayloadsProps) {
    const [open, setOpen] = useState(false);
    const [releases, setReleases] = useState<GitHubRelease[]>([]);
    const [loading, setLoading] = useState(false);
    const [downloading, setDownloading] = useState<number | null>(null);
    const [downloadedFiles, setDownloadedFiles] = useState<Set<string>>(new Set());
    const [storeLoaded, setStoreLoaded] = useState(false);
    const [selectedPayload, setSelectedPayload] = useState<string | null>(null);

    // Load downloaded files from store on mount
    useEffect(() => {
        const loadDownloadedFiles = async () => {
            store = await Store.load("payloads.dat");
            try {
                const stored = await store?.get<string[]>('downloadedFiles');
                if (stored && Array.isArray(stored)) {
                    setDownloadedFiles(new Set(stored));
                }
            } catch (error) {
                console.log('No stored download data found, starting fresh');
            }
            setStoreLoaded(true);
        };
        loadDownloadedFiles();
    }, []);

    // Save downloaded files to store whenever they change
    useEffect(() => {
        if (storeLoaded && downloadedFiles.size > 0) {
            const saveToStore = async () => {
                try {
                    await store?.set('downloadedFiles', Array.from(downloadedFiles));
                    await store?.save();
                } catch (error) {
                    console.error('Failed to save download data:', error);
                }
            };
            saveToStore();
        }
    }, [downloadedFiles, storeLoaded]);

    const checkDownloadedFiles = async (releases: GitHubRelease[]) => {
        const downloaded = new Set<string>();
        const downloadDirPath = await downloadDir();
        const payloadsDir = await join(downloadDirPath, "payloads");

        for (const release of releases) {
            if (release.assets.length > 0) {
                const assetName = release.assets[0].name;
                const filePath = await join(payloadsDir, assetName);
                try {
                    const fileExists = await exists(filePath);
                    if (fileExists) {
                        downloaded.add(assetName);
                    }
                } catch (error) {
                    // File doesn't exist or can't be checked
                }
            }
        }
        setDownloadedFiles(prev => new Set([...prev, ...downloaded]));
    };

    useEffect(() => {
        if (open) {
            setLoading(true);
            async function fetchReleases() {
                const data = await getReleases({
                    owner: 'CTCaer',
                    repo: 'hekate',
                });
                if (data) {
                    setReleases(data);
                    await checkDownloadedFiles(data);
                }
                setLoading(false);
            }
            fetchReleases();
        }
    }, [open]);

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger asChild>
                <Button variant="outline">
                    <DownloadCloud /> fetch
                </Button>
            </DialogTrigger>
            <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
                <DialogHeader>
                    <DialogTitle>fetch payloads</DialogTitle>
                </DialogHeader>
                <div className="space-y-4">
                    {loading || !storeLoaded ? (
                        <div className="text-center py-8">loading releases...</div>
                    ) : releases.length === 0 ? (
                        <div className="text-center py-8 text-muted-foreground">
                            no releases found
                        </div>
                    ) : (
                        releases.map((release, index) => {
                            const isLatest = index === 0;
                            const assetName = release.assets[0]?.name;
                            const isDownloaded = assetName && downloadedFiles.has(assetName);
                            const isInUse = assetName && selectedPayload === assetName;

                            return (
                                <Card key={release.id}>
                                    <CardHeader>
                                        <div className="flex flex-row justify-between">
                                            <div className="flex gap-2 flex-col">
                                            <CardTitle className="text-base inline items-center gap-2">
                                                {release.name || release.tag_name}
                                            </CardTitle>
                                            <CardDescription>
                                            {new Date(release.published_at).toLocaleDateString()}
                                        </CardDescription>
                                            </div>
                                            <div className="flex flex-col items-end gap-2">
                                            {isLatest && (
                                                    <Badge variant="default" className="bg-green-500">recommended</Badge>
                                                )}
                                                <Badge variant="secondary">{release.tag_name}</Badge>
                                                
                                            </div>
                                            
                                        </div>
                                        
                                    </CardHeader>
                                    {release.assets.length > 0 && (
                                        <CardFooter className="flex justify-end gap-2">
                                            {isDownloaded ? (
                                                <Button
                                                    variant="default"
                                                    size="sm"
                                                    className="self-end"
                                                    disabled={!!isInUse}
                                                    onClick={async () => {
                                                        const downloadDirPath = await downloadDir();
                                                        const payloadsDir = await join(downloadDirPath, "payloads");
                                                        const filePath = await join(payloadsDir, assetName);
                                                        setSelectedPayload(assetName);
                                                        onSelectPayload?.(filePath);
                                                        setOpen(false);
                                                        alert(`selected payload: ${assetName}`);
                                                    }}
                                                >
                                                    {isInUse ? <>
                                                        <Check /> in use
                                                    </> : 'use'}
                                                </Button>
                                            ) : (
                                                <Button
                                                    variant="outline"
                                                    size="sm"
                                                    disabled={downloading === release.id}
                                                    onClick={async () => {
                                                        if (!release.assets[0]) return;

                                                        setDownloading(release.id);
                                                        try {
                                                            const filePath = await downloadPayload(
                                                                release.assets[0].browser_download_url,
                                                                release.assets[0].name
                                                            );
                                                            setDownloadedFiles(prev => new Set(prev).add(release.assets[0].name));
                                                            alert(`downloaded to: ${filePath}`);
                                                        } catch (error) {
                                                            alert(`download failed: ${error}`);
                                                        } finally {
                                                            setDownloading(null);
                                                        }
                                                    }}
                                                >
                                                    <DownloadCloud className="w-4 h-4 mr-2" />
                                                    {downloading === release.id ? "downloading..." : "download"}
                                                </Button>
                                            )}
                                        </CardFooter>
                                    )}
                                </Card>
                            );
                        })
                    )}
                </div>
            </DialogContent>
        </Dialog>
    );
}