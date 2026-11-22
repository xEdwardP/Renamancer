import { invoke } from '@tauri-apps/api/core';
import type { Api } from '../types';

export const desktopApi: Api = {
  pickFiles: () => invoke('pick_files'),
  preview: (revision, config) => invoke('generate_preview', { revision, config }),
};
