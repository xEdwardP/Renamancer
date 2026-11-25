import { invoke } from '@tauri-apps/api/core';
import type { Api } from '../types';

export const desktopApi: Api = {
  pickFiles: () => invoke('pick_files'),
  notifyRevision: (request) => invoke('notify_preview_revision', { request }),
  preview: (request) => invoke('generate_preview', { request }),
};
