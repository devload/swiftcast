import { invoke } from '@tauri-apps/api/core';
import { useState, useEffect } from 'react';

interface DashboardProps {
  proxyRunning: boolean;
  activeAccount: any;
  onProxyToggle: () => void;
}

interface AppConfig {
  proxy_port: number;
  auto_start: boolean;
}

export default function Dashboard({ proxyRunning, activeAccount, onProxyToggle }: DashboardProps) {
  const [config, setConfig] = useState<AppConfig>({ proxy_port: 32080, auto_start: true });
  const [editingPort, setEditingPort] = useState(false);
  const [portInput, setPortInput] = useState('32080');

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

  const handleStartProxy = async () => {
    try {
      await invoke('start_proxy', { port: config.proxy_port });
      onProxyToggle();
    } catch (error) {
      console.error('Failed to start proxy:', error);
      alert(`프록시 시작 실패: ${error}`);
    }
  };

  const handleStopProxy = async () => {
    try {
      await invoke('stop_proxy');
      onProxyToggle();
    } catch (error) {
      console.error('Failed to stop proxy:', error);
      alert(`프록시 중지 실패: ${error}`);
    }
  };

  const handlePortChange = async () => {
    const port = parseInt(portInput, 10);
    if (isNaN(port) || port < 1024 || port > 65535) {
      alert('포트는 1024-65535 범위여야 합니다');
      return;
    }

    try {
      await invoke('set_proxy_port', { port });
      setConfig({ ...config, proxy_port: port });
      setEditingPort(false);
    } catch (error) {
      console.error('Failed to set port:', error);
      alert(`포트 변경 실패: ${error}`);
    }
  };

  const handleAutoStartToggle = async () => {
    try {
      const newValue = !config.auto_start;
      await invoke('set_auto_start', { enabled: newValue });
      setConfig({ ...config, auto_start: newValue });
    } catch (error) {
      console.error('Failed to toggle auto start:', error);
    }
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">프록시 제어</h2>

      <div className="space-y-4">
        {/* 활성 계정 정보 */}
        <div className="bg-blue-50 rounded-lg p-4">
          <div className="text-sm text-blue-600 font-medium mb-1">활성 계정</div>
          {activeAccount ? (
            <div>
              <div className="text-lg font-semibold text-gray-900">{activeAccount.name}</div>
              <div className="text-sm text-gray-600 mt-1">{activeAccount.base_url}</div>
            </div>
          ) : (
            <div className="text-gray-500">계정이 없습니다</div>
          )}
        </div>

        {/* 프록시 제어 버튼 */}
        <div className="flex space-x-3">
          {!proxyRunning ? (
            <button
              onClick={handleStartProxy}
              disabled={!activeAccount}
              className="flex-1 bg-green-600 hover:bg-green-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded-lg transition-colors"
            >
              프록시 시작
            </button>
          ) : (
            <button
              onClick={handleStopProxy}
              className="flex-1 bg-red-600 hover:bg-red-700 text-white font-medium py-2 px-4 rounded-lg transition-colors"
            >
              프록시 중지
            </button>
          )}
        </div>

        {/* 프록시 설정 */}
        <div className="bg-gray-50 rounded-lg p-4">
          <div className="grid grid-cols-3 gap-4 text-sm">
            <div>
              <div className="text-gray-500 mb-1">포트</div>
              {editingPort ? (
                <div className="flex items-center space-x-2">
                  <input
                    type="number"
                    value={portInput}
                    onChange={(e) => setPortInput(e.target.value)}
                    className="w-20 px-2 py-1 border rounded text-sm"
                    min="1024"
                    max="65535"
                  />
                  <button
                    onClick={handlePortChange}
                    className="text-green-600 hover:text-green-700 text-xs"
                  >
                    저장
                  </button>
                  <button
                    onClick={() => {
                      setEditingPort(false);
                      setPortInput(config.proxy_port.toString());
                    }}
                    className="text-gray-500 hover:text-gray-700 text-xs"
                  >
                    취소
                  </button>
                </div>
              ) : (
                <div className="flex items-center space-x-2">
                  <span className="font-medium">{config.proxy_port}</span>
                  {!proxyRunning && (
                    <button
                      onClick={() => setEditingPort(true)}
                      className="text-blue-600 hover:text-blue-700 text-xs"
                    >
                      변경
                    </button>
                  )}
                </div>
              )}
            </div>
            <div>
              <div className="text-gray-500 mb-1">상태</div>
              <div className={`font-medium ${proxyRunning ? 'text-green-600' : 'text-gray-600'}`}>
                {proxyRunning ? '실행 중' : '중지됨'}
              </div>
            </div>
            <div>
              <div className="text-gray-500 mb-1">자동 시작</div>
              <label className="flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={config.auto_start}
                  onChange={handleAutoStartToggle}
                  className="sr-only"
                />
                <div className={`w-10 h-5 rounded-full transition-colors ${config.auto_start ? 'bg-green-500' : 'bg-gray-300'}`}>
                  <div className={`w-4 h-4 bg-white rounded-full shadow transform transition-transform mt-0.5 ${config.auto_start ? 'translate-x-5 ml-0.5' : 'translate-x-0.5'}`} />
                </div>
              </label>
            </div>
          </div>
        </div>

        {/* Claude Code 설정 안내 */}
        {proxyRunning && (
          <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
            <div className="text-sm text-yellow-800">
              <div className="font-medium mb-1">Claude Code 설정 (자동 적용됨)</div>
              <div className="text-xs">
                <code className="bg-yellow-100 px-2 py-1 rounded">
                  ~/.claude/settings.json
                </code>
                <div className="mt-2">
                  <pre className="bg-yellow-100 px-2 py-1 rounded overflow-x-auto">
{`{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:${config.proxy_port}"
  }
}`}
                  </pre>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
