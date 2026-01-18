import { invoke } from '@tauri-apps/api/core';
import { useState, useEffect } from 'react';

interface DashboardProps {
  proxyRunning: boolean;
  activeAccount: any;
  onProxyToggle: () => void;
}

interface BackupInfo {
  filename: string;
  timestamp: number;
  size: number;
}

export default function Dashboard({ proxyRunning, activeAccount, onProxyToggle }: DashboardProps) {
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const handleStartProxy = async () => {
    try {
      await invoke('start_proxy', { port: 8080 });
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

  const loadBackups = async () => {
    try {
      const result = await invoke<BackupInfo[]>('list_backups');
      setBackups(result);
    } catch (error) {
      console.error('Failed to load backups:', error);
    }
  };

  const handleBackup = async () => {
    setLoading(true);
    try {
      await invoke('backup_claude_settings');
      alert('Claude 설정이 백업되었습니다.');
      await loadBackups();
    } catch (error) {
      console.error('Failed to backup settings:', error);
      alert(`백업 실패: ${error}`);
    } finally {
      setLoading(false);
    }
  };

  const handleRestore = async (filename: string) => {
    if (!confirm(`백업 파일을 복원하시겠습니까?\n${formatDate(getTimestampFromFilename(filename))}`)) {
      return;
    }

    setLoading(true);
    try {
      await invoke('restore_claude_settings', { backupFilename: filename });
      alert('Claude 설정이 복원되었습니다. Claude Code를 재시작하세요.');
    } catch (error) {
      console.error('Failed to restore settings:', error);
      alert(`복원 실패: ${error}`);
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteBackup = async (filename: string) => {
    if (!confirm(`백업 파일을 삭제하시겠습니까?\n${formatDate(getTimestampFromFilename(filename))}`)) {
      return;
    }

    try {
      await invoke('delete_backup', { backupFilename: filename });
      await loadBackups();
    } catch (error) {
      console.error('Failed to delete backup:', error);
      alert(`백업 삭제 실패: ${error}`);
    }
  };

  const getTimestampFromFilename = (filename: string): number => {
    const match = filename.match(/settings_backup_(\d+)\.json/);
    return match ? parseInt(match[1]) : 0;
  };

  const formatDate = (timestamp: number): string => {
    return new Date(timestamp * 1000).toLocaleString('ko-KR');
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  };

  useEffect(() => {
    loadBackups();
  }, []);

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

        {/* 프록시 정보 */}
        <div className="bg-gray-50 rounded-lg p-4">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <div className="text-gray-500">포트</div>
              <div className="font-medium">8080</div>
            </div>
            <div>
              <div className="text-gray-500">상태</div>
              <div className={`font-medium ${proxyRunning ? 'text-green-600' : 'text-gray-600'}`}>
                {proxyRunning ? '실행 중' : '중지됨'}
              </div>
            </div>
          </div>
        </div>

        {/* Claude Code 설정 안내 */}
        {proxyRunning && (
          <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
            <div className="text-sm text-yellow-800">
              <div className="font-medium mb-1">Claude Code 설정</div>
              <div className="text-xs">
                <code className="bg-yellow-100 px-2 py-1 rounded">
                  %APPDATA%\Claude\settings.json
                </code>
                <div className="mt-2">
                  <pre className="bg-yellow-100 px-2 py-1 rounded overflow-x-auto">
{`{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:8080"
  }
}`}
                  </pre>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Claude 설정 백업/복원 */}
        <div className="border-t pt-4 mt-4">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-md font-semibold text-gray-900">Claude 설정 백업</h3>
            <button
              onClick={handleBackup}
              disabled={loading}
              className="bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white text-sm font-medium py-1.5 px-3 rounded transition-colors"
            >
              {loading ? '처리 중...' : '백업 생성'}
            </button>
          </div>

          {backups.length > 0 ? (
            <div className="space-y-2">
              {backups.map((backup) => (
                <div
                  key={backup.filename}
                  className="bg-gray-50 rounded-lg p-3 flex items-center justify-between"
                >
                  <div className="flex-1">
                    <div className="text-sm font-medium text-gray-900">
                      {formatDate(backup.timestamp)}
                    </div>
                    <div className="text-xs text-gray-500">
                      {formatFileSize(backup.size)}
                    </div>
                  </div>
                  <div className="flex space-x-2">
                    <button
                      onClick={() => handleRestore(backup.filename)}
                      disabled={loading}
                      className="bg-green-600 hover:bg-green-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white text-xs font-medium py-1 px-2 rounded transition-colors"
                    >
                      복원
                    </button>
                    <button
                      onClick={() => handleDeleteBackup(backup.filename)}
                      disabled={loading}
                      className="bg-red-600 hover:bg-red-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white text-xs font-medium py-1 px-2 rounded transition-colors"
                    >
                      삭제
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="bg-gray-50 rounded-lg p-4 text-center text-sm text-gray-500">
              백업 파일이 없습니다
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
