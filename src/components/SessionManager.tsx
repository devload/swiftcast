import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';

interface Account {
  id: string;
  name: string;
  base_url: string;
  created_at: number;
  is_active: boolean;
}

interface SessionDetail {
  session_id: string;
  account_id: string;
  account_name: string;
  model_override: string | null;
  last_message: string | null;
  created_at: number;
  last_activity_at: number;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface ModelInfo {
  id: string;
  name: string;
}

export default function SessionManager() {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<SessionDetail[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, ModelInfo[]>>({});
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    loadData();
    // 5초마다 업데이트 (OOM 방지)
    const interval = setInterval(loadData, 5000);
    return () => clearInterval(interval);
  }, []);

  const loadData = async () => {
    try {
      const [sessionsResult, accountsResult] = await Promise.all([
        invoke<SessionDetail[]>('get_active_sessions'),
        invoke<Account[]>('get_accounts'),
      ]);
      setSessions(sessionsResult);
      setAccounts(accountsResult);

      // Load models for each unique base_url
      const baseUrls = [...new Set(accountsResult.map(a => a.base_url))];
      for (const baseUrl of baseUrls) {
        if (!modelsByProvider[baseUrl]) {
          const models = await invoke<ModelInfo[]>('get_available_models', { baseUrl });
          setModelsByProvider(prev => ({ ...prev, [baseUrl]: models }));
        }
      }
    } catch (error) {
      console.error('Failed to load sessions:', error);
    }
  };

  const handleAccountChange = async (sessionId: string, newAccountId: string) => {
    setLoading(true);
    try {
      const session = sessions.find(s => s.session_id === sessionId);
      await invoke('set_session_config', {
        sessionId,
        accountId: newAccountId,
        modelOverride: session?.model_override || null,
      });
      await loadData();
    } catch (error) {
      console.error('Failed to update session account:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleModelChange = async (sessionId: string, modelId: string | null) => {
    setLoading(true);
    try {
      const session = sessions.find(s => s.session_id === sessionId);
      if (!session) return;
      await invoke('set_session_config', {
        sessionId,
        accountId: session.account_id,
        modelOverride: modelId || null,
      });
      await loadData();
    } catch (error) {
      console.error('Failed to update session model:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    if (!confirm(t('sessions.confirmDelete'))) return;

    setLoading(true);
    try {
      await invoke('delete_session_config', { sessionId });
      await loadData();
    } catch (error) {
      console.error('Failed to delete session:', error);
    } finally {
      setLoading(false);
    }
  };

  const formatRelativeTime = (timestamp: number): string => {
    const now = Math.floor(Date.now() / 1000);
    const diff = now - timestamp;

    if (diff < 60) return t('sessions.justNow');
    if (diff < 3600) return t('sessions.minutesAgo', { minutes: Math.floor(diff / 60) });
    if (diff < 86400) return t('sessions.hoursAgo', { hours: Math.floor(diff / 3600) });
    return new Date(timestamp * 1000).toLocaleDateString();
  };

  const getModelsForAccount = (accountId: string): ModelInfo[] => {
    const account = accounts.find(a => a.id === accountId);
    if (!account) return [];
    return modelsByProvider[account.base_url] || [];
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h2 className="text-lg font-semibold text-gray-900">{t('sessions.title')}</h2>
          <p className="text-sm text-gray-500">
            {t('sessions.activeSessions')}: {sessions.length}
          </p>
        </div>
        <button
          onClick={loadData}
          disabled={loading}
          className="bg-gray-100 hover:bg-gray-200 disabled:bg-gray-50 text-gray-700 text-sm font-medium py-2 px-4 rounded-lg transition-colors"
        >
          {loading ? '...' : t('sessions.refresh')}
        </button>
      </div>

      {sessions.length === 0 ? (
        <div className="text-center text-gray-500 py-8">
          {t('sessions.noSessions')}
        </div>
      ) : (
        <div className="space-y-3">
          {sessions.map((session) => (
            <div
              key={session.session_id}
              className="border border-gray-200 rounded-lg p-4 hover:bg-gray-50"
            >
              {/* Session Header */}
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center">
                  <span className="font-mono text-xs text-gray-400">
                    {session.session_id.substring(0, 8)}
                  </span>
                  <span className="mx-2 text-gray-300">·</span>
                  <span className="text-xs text-gray-400">
                    {formatRelativeTime(session.last_activity_at)}
                  </span>
                </div>
                <button
                  onClick={() => handleDeleteSession(session.session_id)}
                  className="text-red-600 hover:text-red-700 text-sm font-medium"
                >
                  {t('sessions.delete')}
                </button>
              </div>

              {/* Last Message */}
              {session.last_message && (
                <div className="mb-3 p-2 bg-gray-50 rounded-md">
                  <p className="text-sm text-gray-700 line-clamp-2" title={session.last_message}>
                    "{session.last_message}"
                  </p>
                </div>
              )}

              {/* Vendor & Model Selection */}
              <div className="grid grid-cols-2 gap-3 mb-3">
                <div>
                  <label className="block text-xs font-medium text-gray-500 mb-1">
                    {t('sessions.vendor')}
                  </label>
                  <select
                    value={session.account_id}
                    onChange={(e) => handleAccountChange(session.session_id, e.target.value)}
                    disabled={loading}
                    className="w-full px-3 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
                  >
                    {accounts.map((account) => (
                      <option key={account.id} value={account.id}>
                        {account.name}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-xs font-medium text-gray-500 mb-1">
                    {t('sessions.model')}
                  </label>
                  <select
                    value={session.model_override || ''}
                    onChange={(e) => handleModelChange(session.session_id, e.target.value || null)}
                    disabled={loading}
                    className="w-full px-3 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
                  >
                    <option value="">{t('sessions.keepOriginal')}</option>
                    {getModelsForAccount(session.account_id).map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.name}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              {/* Session Stats */}
              <div className="flex items-center text-xs text-gray-500 space-x-4">
                <span>
                  {t('sessions.requests')}: {session.request_count.toLocaleString()}
                </span>
                <span>
                  {t('sessions.tokens')}: {(session.total_input_tokens + session.total_output_tokens).toLocaleString()}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
