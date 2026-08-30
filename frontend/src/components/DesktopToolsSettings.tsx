'use client';

import { useCallback, useEffect, useState } from 'react';
import { CheckCircle2, Code2, Download, Loader2, Terminal, XCircle } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

type InstallState = 'checking' | 'idle' | 'installing' | 'ready' | 'error';

interface NodeRuntimeStatus {
  nodeAvailable: boolean;
  npmAvailable: boolean;
  managedRuntimeAvailable: boolean;
  version: string;
}

interface OpenSpecStatus {
  openspecAvailable: boolean;
}

function StatusIcon({ state }: { state: InstallState }) {
  if (state === 'installing' || state === 'checking') {
    return <Loader2 className="h-5 w-5 animate-spin text-blue-600" />;
  }
  if (state === 'ready') {
    return <CheckCircle2 className="h-5 w-5 text-emerald-600" />;
  }
  if (state === 'error') {
    return <XCircle className="h-5 w-5 text-red-600" />;
  }
  return <Download className="h-5 w-5 text-gray-500" />;
}

/**
 * Desktop-only prerequisites, intentionally independent from model settings.
 * Node/npm is installed as an app-managed portable runtime; OpenSpec is then
 * installed into that exact runtime. No global PATH edit, admin prompt, or
 * Windows restart is necessary.
 */
export function DesktopToolsSettings() {
  const [nodeStatus, setNodeStatus] = useState<NodeRuntimeStatus | null>(null);
  const [openSpecStatus, setOpenSpecStatus] = useState<OpenSpecStatus | null>(null);
  const [nodeState, setNodeState] = useState<InstallState>('checking');
  const [openSpecState, setOpenSpecState] = useState<InstallState>('checking');
  const [nodeError, setNodeError] = useState<string | null>(null);
  const [openSpecError, setOpenSpecError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [runtime, cli] = await Promise.all([
        invoke<NodeRuntimeStatus>('check_node_runtime_status'),
        invoke<OpenSpecStatus>('check_openspec_setup_status'),
      ]);
      setNodeStatus(runtime);
      setOpenSpecStatus(cli);
      setNodeState(runtime.managedRuntimeAvailable && runtime.nodeAvailable && runtime.npmAvailable ? 'ready' : 'idle');
      setOpenSpecState(cli.openspecAvailable ? 'ready' : 'idle');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNodeError(message);
      setOpenSpecError(message);
      setNodeState('error');
      setOpenSpecState('error');
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const installNode = async () => {
    setNodeState('installing');
    setNodeError(null);
    try {
      await invoke('install_node_runtime');
      await refresh();
    } catch (error) {
      setNodeError(error instanceof Error ? error.message : String(error));
      setNodeState('error');
    }
  };

  const installOpenSpec = async () => {
    setOpenSpecState('installing');
    setOpenSpecError(null);
    try {
      await invoke('install_openspec_cli');
      await refresh();
    } catch (error) {
      setOpenSpecError(error instanceof Error ? error.message : String(error));
      setOpenSpecState('error');
    }
  };

  const nodeReady = nodeState === 'ready';
  const openSpecReady = openSpecState === 'ready';

  return (
    <div className="mt-6 space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-gray-900">Herramientas de especificación</h2>
        <p className="mt-1 text-sm text-gray-600">
          Instalá los requisitos de OpenSpec en pasos separados. Meet4Specs usa un runtime privado de la app,
          así que no modifica tu PATH global ni requiere reiniciar Windows.
        </p>
      </div>

      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-start justify-between gap-4">
          <div className="flex gap-3">
            <Terminal className="mt-0.5 h-6 w-6 text-blue-600" />
            <div>
              <h3 className="font-semibold text-gray-900">Node.js y npm</h3>
              <p className="mt-1 text-sm text-gray-600">
                Runtime portable administrado por Meet4Specs. OpenSpec requiere Node.js 20.19.0 o superior.
              </p>
              {nodeReady && <p className="mt-2 text-sm text-emerald-700">Listo: Node.js {nodeStatus?.version} y npm están disponibles.</p>}
              {nodeError && <p className="mt-2 break-words text-sm text-red-700">{nodeError}</p>}
            </div>
          </div>
          <StatusIcon state={nodeState} />
        </div>
        {!nodeReady && (
          <button
            type="button"
            onClick={() => void installNode()}
            disabled={nodeState === 'installing'}
            className="mt-5 inline-flex items-center rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {nodeState === 'installing' ? 'Instalando Node.js...' : nodeState === 'error' ? 'Reintentar instalación' : 'Instalar Node.js y npm'}
          </button>
        )}
      </section>

      <section className="rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
        <div className="flex items-start justify-between gap-4">
          <div className="flex gap-3">
            <Code2 className="mt-0.5 h-6 w-6 text-violet-600" />
            <div>
              <h3 className="font-semibold text-gray-900">OpenSpec CLI</h3>
              <p className="mt-1 text-sm text-gray-600">
                Genera propuestas y especificaciones desde las transcripciones de reuniones.
              </p>
              {openSpecReady && <p className="mt-2 text-sm text-emerald-700">Listo: OpenSpec CLI responde correctamente.</p>}
              {!nodeReady && <p className="mt-2 text-sm text-amber-700">Primero instalá el runtime Node.js y npm de arriba.</p>}
              {openSpecError && <p className="mt-2 break-words text-sm text-red-700">{openSpecError}</p>}
            </div>
          </div>
          <StatusIcon state={openSpecState} />
        </div>
        {!openSpecReady && (
          <button
            type="button"
            onClick={() => void installOpenSpec()}
            disabled={!nodeReady || openSpecState === 'installing'}
            className="mt-5 inline-flex items-center rounded-md bg-violet-600 px-3 py-2 text-sm font-medium text-white hover:bg-violet-700 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {openSpecState === 'installing' ? 'Instalando OpenSpec...' : openSpecState === 'error' ? 'Reintentar instalación' : 'Instalar OpenSpec CLI'}
          </button>
        )}
      </section>
    </div>
  );
}
