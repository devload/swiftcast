import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface UsageStats {
  request_count: number;
  input_tokens: number;
  output_tokens: number;
}

interface ModelUsageStats {
  model: string;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface DailyUsageStats {
  date: string;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

interface UsageLog {
  id: number;
  timestamp: number;
  account_id: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  session_id?: string;
}

interface SessionUsageStats {
  session_id: string;
  first_request: number;
  last_request: number;
  request_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

// 툴팁 컴포넌트
function Tooltip({ text, children }: { text: string; children: React.ReactNode }) {
  return (
    <span className="group relative inline-flex items-center">
      {children}
      <span className="invisible group-hover:visible absolute left-1/2 -translate-x-1/2 top-full mt-1 px-2 py-1 bg-gray-800 text-white text-xs rounded whitespace-nowrap z-10">
        {text}
        <span className="absolute bottom-full left-1/2 -translate-x-1/2 border-4 border-transparent border-b-gray-800"></span>
      </span>
    </span>
  );
}

export default function UsageMonitor() {
  const [stats, setStats] = useState<UsageStats>({
    request_count: 0,
    input_tokens: 0,
    output_tokens: 0,
  });
  const [modelStats, setModelStats] = useState<ModelUsageStats[]>([]);
  const [dailyStats, setDailyStats] = useState<DailyUsageStats[]>([]);
  const [sessionStats, setSessionStats] = useState<SessionUsageStats[]>([]);
  const [recentLogs, setRecentLogs] = useState<UsageLog[]>([]);
  const [activeTab, setActiveTab] = useState<'overview' | 'models' | 'daily' | 'sessions' | 'logs'>('overview');

  useEffect(() => {
    loadStats();

    // 자동 업데이트: 2초마다 통계 확인
    const interval = setInterval(() => {
      loadStats();
    }, 2000);

    return () => clearInterval(interval);
  }, []);

  const loadStats = async () => {
    try {
      const [statsResult, modelResult, dailyResult, sessionResult, logsResult] = await Promise.all([
        invoke('get_usage_stats'),
        invoke('get_usage_by_model'),
        invoke('get_daily_usage', { days: 7 }),
        invoke('get_usage_by_session'),
        invoke('get_recent_usage', { limit: 10 }),
      ]);
      setStats(statsResult as UsageStats);
      setModelStats(modelResult as ModelUsageStats[]);
      setDailyStats(dailyResult as DailyUsageStats[]);
      setSessionStats(sessionResult as SessionUsageStats[]);
      setRecentLogs(logsResult as UsageLog[]);
    } catch (error) {
      console.error('Failed to load usage stats:', error);
    }
  };

  const formatNumber = (num: number) => {
    return new Intl.NumberFormat('ko-KR').format(num);
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString('ko-KR');
  };

  const tabs = [
    { id: 'overview', label: '개요' },
    { id: 'models', label: '모델별' },
    { id: 'daily', label: '일별' },
    { id: 'sessions', label: '세션별' },
    { id: 'logs', label: '최근 로그' },
  ] as const;

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">사용량 모니터링</h2>

      {/* 탭 네비게이션 */}
      <div className="flex space-x-1 mb-4 bg-gray-100 rounded-lg p-1">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 px-3 py-2 text-sm font-medium rounded-md transition-colors ${
              activeTab === tab.id
                ? 'bg-white text-gray-900 shadow'
                : 'text-gray-600 hover:text-gray-900'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* 개요 탭 */}
      {activeTab === 'overview' && (
        <div className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="bg-blue-50 rounded-lg p-4">
              <Tooltip text="Claude API에 보낸 총 요청 횟수">
                <div className="text-sm text-blue-600 font-medium mb-1 cursor-help">
                  요청 수 ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.request_count)}</div>
            </div>

            <div className="bg-green-50 rounded-lg p-4">
              <Tooltip text="시스템 프롬프트 + 대화 히스토리 + 사용자 메시지에 사용된 토큰">
                <div className="text-sm text-green-600 font-medium mb-1 cursor-help">
                  입력 토큰 ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.input_tokens)}</div>
            </div>

            <div className="bg-purple-50 rounded-lg p-4">
              <Tooltip text="Claude가 생성한 응답에 사용된 토큰">
                <div className="text-sm text-purple-600 font-medium mb-1 cursor-help">
                  출력 토큰 ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.output_tokens)}</div>
            </div>
          </div>

          <div className="bg-gray-50 rounded-lg p-4">
            <Tooltip text="입력 토큰 + 출력 토큰 = 총 사용량 (비용 계산의 기준)">
              <span className="text-sm text-gray-600 cursor-help">
                총 토큰 ⓘ
              </span>
            </Tooltip>
            <span className="text-sm text-gray-600">: <span className="font-semibold">{formatNumber(stats.input_tokens + stats.output_tokens)}</span></span>
          </div>
        </div>
      )}

      {/* 모델별 탭 */}
      {activeTab === 'models' && (
        <div className="space-y-2">
          {modelStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">데이터 없음</div>
          ) : (
            modelStats.map((model, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{model.model || 'unknown'}</div>
                    <div className="text-sm text-gray-500">{model.request_count}회 요청</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      입력: {formatNumber(model.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      출력: {formatNumber(model.total_output_tokens)}
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 일별 탭 */}
      {activeTab === 'daily' && (
        <div className="space-y-2">
          {dailyStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">데이터 없음</div>
          ) : (
            dailyStats.map((day, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{day.date}</div>
                    <div className="text-sm text-gray-500">{day.request_count}회 요청</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      입력: {formatNumber(day.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      출력: {formatNumber(day.total_output_tokens)}
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 세션별 탭 */}
      {activeTab === 'sessions' && (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {sessionStats.length === 0 ? (
            <div className="text-center text-gray-500 py-8">데이터 없음</div>
          ) : (
            sessionStats.map((session, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-start">
                  <div className="flex-1 min-w-0">
                    <Tooltip text={`세션 ID: ${session.session_id}`}>
                      <div className="font-mono text-xs text-gray-500 truncate cursor-help">
                        {session.session_id.substring(0, 12)}...
                      </div>
                    </Tooltip>
                    <div className="text-sm text-gray-600 mt-1">
                      {formatDate(session.first_request)} ~ {formatDate(session.last_request)}
                    </div>
                    <div className="text-sm text-gray-500">{session.request_count}회 요청</div>
                  </div>
                  <div className="text-right ml-4">
                    <div className="text-sm text-gray-600">
                      입력: {formatNumber(session.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      출력: {formatNumber(session.total_output_tokens)}
                    </div>
                    <div className="text-xs text-gray-400 mt-1">
                      총 {formatNumber(session.total_input_tokens + session.total_output_tokens)} 토큰
                    </div>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* 최근 로그 탭 */}
      {activeTab === 'logs' && (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {recentLogs.length === 0 ? (
            <div className="text-center text-gray-500 py-8">데이터 없음</div>
          ) : (
            recentLogs.map((log) => (
              <div key={log.id} className="bg-gray-50 rounded-lg p-3 text-sm">
                <div className="flex justify-between">
                  <span className="font-medium">{log.model || 'unknown'}</span>
                  <span className="text-gray-500">{formatDate(log.timestamp)}</span>
                </div>
                <div className="text-gray-600 mt-1">
                  입력: {formatNumber(log.input_tokens)} / 출력: {formatNumber(log.output_tokens)}
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
