import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';

interface AppConfig {
  proxy_port: number;
  auto_start: boolean;
}

interface HookConfig {
  hooks_enabled: boolean;
  hooks_retention_days: number;
  compaction_injection_enabled: boolean;
  compaction_summarization_instructions: string;
  compaction_context_injection: string;
}

const LANGUAGES = [
  { code: 'ko', name: '한국어' },
  { code: 'en', name: 'English' },
  { code: 'ja', name: '日本語' },
  { code: 'zh', name: '中文' },
];

export default function Settings() {
  const { t, i18n } = useTranslation();
  const [config, setConfig] = useState<AppConfig>({ proxy_port: 32080, auto_start: true });
  const [hookConfig, setHookConfig] = useState<HookConfig>({
    hooks_enabled: true,
    hooks_retention_days: 30,
    compaction_injection_enabled: false,
    compaction_summarization_instructions: '',
    compaction_context_injection: '',
  });
  const [editingPort, setEditingPort] = useState(false);
  const [portInput, setPortInput] = useState('32080');
  const [saving, setSaving] = useState(false);
  const [savingHook, setSavingHook] = useState(false);
  const [appVersion, setAppVersion] = useState('');

  useEffect(() => {
    loadConfig();
    loadHookConfig();
    loadAppVersion();
  }, []);

  const loadConfig = async () => {
    try {
      const appConfig = await invoke<AppConfig>('get_app_config');
      setConfig(appConfig);
      setPortInput(appConfig.proxy_port.toString());
    } catch (error) {
      console.error('Failed to load config:', error);
    }
  };

  const loadHookConfig = async () => {
    try {
      const config = await invoke<HookConfig>('get_hook_config');
      setHookConfig(config);
    } catch (error) {
      console.error('Failed to load hook config:', error);
    }
  };

  const loadAppVersion = async () => {
    try {
      const version = await invoke<string>('get_app_version');
      setAppVersion(version);
    } catch (error) {
      console.error('Failed to load app version:', error);
    }
  };

  const handlePortSave = async () => {
    const port = parseInt(portInput, 10);
    if (isNaN(port) || port < 1024 || port > 65535) {
      alert(t('settings.portRangeError'));
      return;
    }

    setSaving(true);
    try {
      await invoke('set_proxy_port', { port });
      setConfig({ ...config, proxy_port: port });
      setEditingPort(false);
    } catch (error) {
      console.error('Failed to set port:', error);
      alert(`${t('settings.portChangeFailed')}: ${error}`);
    } finally {
      setSaving(false);
    }
  };

  const handleAutoStartToggle = async () => {
    setSaving(true);
    try {
      const newValue = !config.auto_start;
      await invoke('set_auto_start', { enabled: newValue });
      setConfig({ ...config, auto_start: newValue });
    } catch (error) {
      console.error('Failed to toggle auto start:', error);
    } finally {
      setSaving(false);
    }
  };

  const handleHookConfigSave = async () => {
    setSavingHook(true);
    try {
      await invoke('set_hook_config', { config: hookConfig });
    } catch (error) {
      console.error('Failed to save hook config:', error);
      alert(`Failed to save hook config: ${error}`);
    } finally {
      setSavingHook(false);
    }
  };

  const handleClearUsage = async () => {
    if (!confirm(t('settings.confirmClearUsage'))) {
      return;
    }

    try {
      await invoke('clear_usage_logs');
      alert(t('settings.usageCleared'));
    } catch (error) {
      console.error('Failed to clear usage:', error);
      alert(`${t('settings.clearFailed')}: ${error}`);
    }
  };

  const handleLanguageChange = (langCode: string) => {
    i18n.changeLanguage(langCode);
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-lg font-semibold text-gray-900">{t('settings.title')}</h2>
        <span className="text-xs text-gray-400 font-mono">v{appVersion}</span>
      </div>

      <div className="space-y-6">
        {/* 언어 설정 */}
        <div className="flex items-center justify-between py-3 border-b">
          <div>
            <div className="font-medium text-gray-900">{t('settings.language')}</div>
            <div className="text-sm text-gray-500">{t('settings.languageDescription')}</div>
          </div>
          <select
            value={i18n.language}
            onChange={(e) => handleLanguageChange(e.target.value)}
            className="px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white appearance-none cursor-pointer"
            style={{
              backgroundImage: `url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b7280' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e")`,
              backgroundPosition: 'right 0.5rem center',
              backgroundRepeat: 'no-repeat',
              backgroundSize: '1.5em 1.5em',
              paddingRight: '2.5rem'
            }}
          >
            {LANGUAGES.map((lang) => (
              <option key={lang.code} value={lang.code}>
                {lang.name}
              </option>
            ))}
          </select>
        </div>

        {/* 프록시 포트 설정 */}
        <div className="flex items-center justify-between py-3 border-b">
          <div>
            <div className="font-medium text-gray-900">{t('settings.proxyPort')}</div>
            <div className="text-sm text-gray-500">{t('settings.proxyPortDescription')}</div>
          </div>
          {editingPort ? (
            <div className="flex items-center space-x-2">
              <input
                type="number"
                value={portInput}
                onChange={(e) => setPortInput(e.target.value)}
                className="w-24 px-3 py-1 border rounded text-sm"
                min="1024"
                max="65535"
                disabled={saving}
              />
              <button
                onClick={handlePortSave}
                disabled={saving}
                className="px-3 py-1 bg-blue-600 text-white rounded text-sm hover:bg-blue-700 disabled:bg-gray-400"
              >
                {t('settings.save')}
              </button>
              <button
                onClick={() => {
                  setEditingPort(false);
                  setPortInput(config.proxy_port.toString());
                }}
                className="px-3 py-1 bg-gray-200 text-gray-700 rounded text-sm hover:bg-gray-300"
              >
                {t('settings.cancel')}
              </button>
            </div>
          ) : (
            <div className="flex items-center space-x-2">
              <span className="font-mono text-gray-900">{config.proxy_port}</span>
              <button
                onClick={() => setEditingPort(true)}
                className="px-3 py-1 bg-gray-100 text-gray-700 rounded text-sm hover:bg-gray-200"
              >
                {t('settings.change')}
              </button>
            </div>
          )}
        </div>

        {/* 자동 시작 설정 */}
        <div className="flex items-center justify-between py-3 border-b">
          <div>
            <div className="font-medium text-gray-900">{t('settings.autoStart')}</div>
            <div className="text-sm text-gray-500">{t('settings.autoStartDescription')}</div>
          </div>
          <label className="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              checked={config.auto_start}
              onChange={handleAutoStartToggle}
              disabled={saving}
              className="sr-only peer"
            />
            <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
          </label>
        </div>

        {/* Hook 설정 섹션 */}
        <div className="py-3 border-b">
          <div className="font-medium text-gray-900 mb-3">Hook System</div>

          {/* Hook 활성화 */}
          <div className="flex items-center justify-between py-2">
            <div>
              <div className="text-sm text-gray-700">API Logging</div>
              <div className="text-xs text-gray-500">Log all requests/responses to ~/.sessioncast/logs/</div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={hookConfig.hooks_enabled}
                onChange={(e) => setHookConfig({ ...hookConfig, hooks_enabled: e.target.checked })}
                className="sr-only peer"
              />
              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
            </label>
          </div>

          {/* 보관 기간 */}
          <div className="flex items-center justify-between py-2">
            <div>
              <div className="text-sm text-gray-700">Retention Days</div>
              <div className="text-xs text-gray-500">Auto-delete logs older than this</div>
            </div>
            <input
              type="number"
              value={hookConfig.hooks_retention_days}
              onChange={(e) => setHookConfig({ ...hookConfig, hooks_retention_days: parseInt(e.target.value) || 30 })}
              className="w-20 px-2 py-1 border rounded text-sm text-right"
              min="1"
              max="365"
            />
          </div>

          {/* Compaction Injection 활성화 */}
          <div className="flex items-center justify-between py-2 mt-3 pt-3 border-t border-gray-100">
            <div>
              <div className="text-sm text-gray-700">Compaction Injection</div>
              <div className="text-xs text-gray-500">Inject context during conversation compaction</div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={hookConfig.compaction_injection_enabled}
                onChange={(e) => setHookConfig({ ...hookConfig, compaction_injection_enabled: e.target.checked })}
                className="sr-only peer"
              />
              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
            </label>
          </div>

          {hookConfig.compaction_injection_enabled && (
            <>
              {/* Summarization Instructions */}
              <div className="py-2">
                <div className="text-sm text-gray-700 mb-1">Summarization Instructions</div>
                <div className="text-xs text-gray-500 mb-2">Added to summary generation prompt</div>
                <textarea
                  value={hookConfig.compaction_summarization_instructions}
                  onChange={(e) => setHookConfig({ ...hookConfig, compaction_summarization_instructions: e.target.value })}
                  placeholder="e.g., Always include: This project uses Korean language"
                  className="w-full px-3 py-2 border rounded text-sm h-20 resize-none"
                />
              </div>

              {/* Context Injection */}
              <div className="py-2">
                <div className="text-sm text-gray-700 mb-1">Context Injection</div>
                <div className="text-xs text-gray-500 mb-2">Injected into compacted conversations</div>
                <textarea
                  value={hookConfig.compaction_context_injection}
                  onChange={(e) => setHookConfig({ ...hookConfig, compaction_context_injection: e.target.value })}
                  placeholder="e.g., Project Rules: Respond in Korean, Use TypeScript strict mode"
                  className="w-full px-3 py-2 border rounded text-sm h-20 resize-none"
                />
              </div>
            </>
          )}

          {/* Hook 설정 저장 버튼 */}
          <div className="pt-3">
            <button
              onClick={handleHookConfigSave}
              disabled={savingHook}
              className="px-4 py-2 bg-blue-600 text-white rounded text-sm hover:bg-blue-700 disabled:bg-gray-400"
            >
              {savingHook ? 'Saving...' : 'Save Hook Settings'}
            </button>
          </div>
        </div>

        {/* Claude Code 설정 경로 */}
        <div className="py-3 border-b">
          <div className="font-medium text-gray-900 mb-2">{t('settings.claudeSettingsFile')}</div>
          <div className="bg-gray-50 rounded p-3">
            <code className="text-sm text-gray-700">~/.claude/settings.json</code>
          </div>
          <div className="mt-2 text-sm text-gray-500">
            {t('settings.claudeSettingsDescription')}
          </div>
        </div>

        {/* 데이터 관리 */}
        <div className="py-3">
          <div className="font-medium text-gray-900 mb-2">{t('settings.dataManagement')}</div>
          <button
            onClick={handleClearUsage}
            className="px-4 py-2 bg-red-100 text-red-700 rounded hover:bg-red-200 text-sm"
          >
            {t('settings.clearUsage')}
          </button>
        </div>
      </div>
    </div>
  );
}
