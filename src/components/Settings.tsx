import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface AppConfig {
  proxy_port: number;
  auto_start: boolean;
}

export default function Settings() {
  const [config, setConfig] = useState<AppConfig>({ proxy_port: 32080, auto_start: true });
  const [editingPort, setEditingPort] = useState(false);
  const [portInput, setPortInput] = useState('32080');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    loadConfig();
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

  const handlePortSave = async () => {
    const port = parseInt(portInput, 10);
    if (isNaN(port) || port < 1024 || port > 65535) {
      alert('포트는 1024-65535 범위여야 합니다');
      return;
    }

    setSaving(true);
    try {
      await invoke('set_proxy_port', { port });
      setConfig({ ...config, proxy_port: port });
      setEditingPort(false);
    } catch (error) {
      console.error('Failed to set port:', error);
      alert(`포트 변경 실패: ${error}`);
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

  const handleClearUsage = async () => {
    if (!confirm('모든 사용량 로그를 삭제하시겠습니까?')) {
      return;
    }

    try {
      await invoke('clear_usage_logs');
      alert('사용량 로그가 삭제되었습니다');
    } catch (error) {
      console.error('Failed to clear usage:', error);
      alert(`삭제 실패: ${error}`);
    }
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">설정</h2>

      <div className="space-y-6">
        {/* 프록시 포트 설정 */}
        <div className="flex items-center justify-between py-3 border-b">
          <div>
            <div className="font-medium text-gray-900">프록시 포트</div>
            <div className="text-sm text-gray-500">Claude Code가 연결할 로컬 프록시 포트</div>
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
                저장
              </button>
              <button
                onClick={() => {
                  setEditingPort(false);
                  setPortInput(config.proxy_port.toString());
                }}
                className="px-3 py-1 bg-gray-200 text-gray-700 rounded text-sm hover:bg-gray-300"
              >
                취소
              </button>
            </div>
          ) : (
            <div className="flex items-center space-x-2">
              <span className="font-mono text-gray-900">{config.proxy_port}</span>
              <button
                onClick={() => setEditingPort(true)}
                className="px-3 py-1 bg-gray-100 text-gray-700 rounded text-sm hover:bg-gray-200"
              >
                변경
              </button>
            </div>
          )}
        </div>

        {/* 자동 시작 설정 */}
        <div className="flex items-center justify-between py-3 border-b">
          <div>
            <div className="font-medium text-gray-900">자동 시작</div>
            <div className="text-sm text-gray-500">앱 실행 시 프록시 자동 시작</div>
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

        {/* Claude Code 설정 경로 */}
        <div className="py-3 border-b">
          <div className="font-medium text-gray-900 mb-2">Claude Code 설정 파일</div>
          <div className="bg-gray-50 rounded p-3">
            <code className="text-sm text-gray-700">~/.claude/settings.json</code>
          </div>
          <div className="mt-2 text-sm text-gray-500">
            계정 전환 시 자동으로 업데이트됩니다
          </div>
        </div>

        {/* 데이터 관리 */}
        <div className="py-3">
          <div className="font-medium text-gray-900 mb-2">데이터 관리</div>
          <button
            onClick={handleClearUsage}
            className="px-4 py-2 bg-red-100 text-red-700 rounded hover:bg-red-200 text-sm"
          >
            사용량 로그 초기화
          </button>
        </div>
      </div>
    </div>
  );
}
