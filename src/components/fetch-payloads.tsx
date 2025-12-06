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
import { Item, ItemContent, ItemTitle, ItemDescription, ItemActions } from './ui/item';
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
                    owner: 'CTCaer', // change these
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
                         <div className="text-center py-8">Loading releases...</div>
                     ) : releases.length === 0 ? (
                         <div className="text-center py-8 text-muted-foreground">
                             No releases found
                         </div>
                     ) : (
                         releases.map((release) => (
                             <Item key={release.id} variant="outline">
                                 <ItemContent>
                                     <div className="flex items-center gap-2 mb-1">
                                         <ItemTitle className="text-base flex items-center gap-2">
                                             {release.name || release.tag_name}
                                             {release.assets.length > 0 && downloadedFiles.has(release.assets[0].name) && (
                                                 <Check className="w-4 h-4 text-green-600" />
                                             )}
                                         </ItemTitle>
                                         <Badge variant="secondary">{release.tag_name}</Badge>
                                     </div>
                                     <ItemDescription>
                                         {new Date(release.published_at).toLocaleDateString()}
                                     </ItemDescription>
                                 </ItemContent>
                                 {release.assets.length > 0 && (
                                     <ItemActions>
                                         {downloadedFiles.has(release.assets[0].name) ? (
                                             <Button
                                                 variant="default"
                                                 size="sm"
                                                 onClick={async () => {
                                                     const downloadDirPath = await downloadDir();
                                                     const payloadsDir = await join(downloadDirPath, "payloads");
                                                     const filePath = await join(payloadsDir, release.assets[0].name);
                                                     onSelectPayload?.(filePath);
                                                     setOpen(false);
                                                     alert(`Selected payload: ${release.assets[0].name}`);
                                                 }}
                                             >
                                                 use
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
                                                         // Update downloaded files state
                                                         setDownloadedFiles(prev => new Set(prev).add(release.assets[0].name));
                                                         alert(`Downloaded to: ${filePath}`);
                                                     } catch (error) {
                                                         alert(`Download failed: ${error}`);
                                                     } finally {
                                                         setDownloading(null);
                                                     }
                                                 }}
                                             >
                                                 <DownloadCloud className="w-4 h-4 mr-2" />
                                                 {downloading === release.id ? "downloading..." : "download"}
                                             </Button>
                                         )}
                                     </ItemActions>
                                 )}
                             </Item>
                         ))
                     )}
                </div>
            </DialogContent>
        </Dialog>
    );
}