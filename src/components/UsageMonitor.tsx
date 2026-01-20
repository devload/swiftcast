import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
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
  const { t } = useTranslation();
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
    return new Intl.NumberFormat().format(num);
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  const tabs = [
    { id: 'overview', label: t('usage.tabs.overview') },
    { id: 'models', label: t('usage.tabs.models') },
    { id: 'daily', label: t('usage.tabs.daily') },
    { id: 'sessions', label: t('usage.tabs.sessions') },
    { id: 'logs', label: t('usage.tabs.logs') },
  ] as const;

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">{t('usage.title')}</h2>

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
              <Tooltip text={t('usage.requestCountTooltip')}>
                <div className="text-sm text-blue-600 font-medium mb-1 cursor-help">
                  {t('usage.requestCount')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.request_count)}</div>
            </div>

            <div className="bg-green-50 rounded-lg p-4">
              <Tooltip text={t('usage.inputTokensTooltip')}>
                <div className="text-sm text-green-600 font-medium mb-1 cursor-help">
                  {t('usage.inputTokens')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.input_tokens)}</div>
            </div>

            <div className="bg-purple-50 rounded-lg p-4">
              <Tooltip text={t('usage.outputTokensTooltip')}>
                <div className="text-sm text-purple-600 font-medium mb-1 cursor-help">
                  {t('usage.outputTokens')} ⓘ
                </div>
              </Tooltip>
              <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.output_tokens)}</div>
            </div>
          </div>

          <div className="bg-gray-50 rounded-lg p-4">
            <Tooltip text={t('usage.totalTokensTooltip')}>
              <span className="text-sm text-gray-600 cursor-help">
                {t('usage.totalTokens')} ⓘ
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
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            modelStats.map((model, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{model.model || 'unknown'}</div>
                    <div className="text-sm text-gray-500">{model.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(model.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(model.total_output_tokens)}
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
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            dailyStats.map((day, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-medium text-gray-900">{day.date}</div>
                    <div className="text-sm text-gray-500">{day.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(day.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(day.total_output_tokens)}
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
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            sessionStats.map((session, idx) => (
              <div key={idx} className="bg-gray-50 rounded-lg p-4">
                <div className="flex justify-between items-start">
                  <div className="flex-1 min-w-0">
                    <Tooltip text={`${t('usage.sessionId')}: ${session.session_id}`}>
                      <div className="font-mono text-xs text-gray-500 truncate cursor-help">
                        {session.session_id.substring(0, 12)}...
                      </div>
                    </Tooltip>
                    <div className="text-sm text-gray-600 mt-1">
                      {formatDate(session.first_request)} ~ {formatDate(session.last_request)}
                    </div>
                    <div className="text-sm text-gray-500">{session.request_count}{t('usage.requests')}</div>
                  </div>
                  <div className="text-right ml-4">
                    <div className="text-sm text-gray-600">
                      {t('usage.input')}: {formatNumber(session.total_input_tokens)}
                    </div>
                    <div className="text-sm text-gray-600">
                      {t('usage.output')}: {formatNumber(session.total_output_tokens)}
                    </div>
                    <div className="text-xs text-gray-400 mt-1">
                      {t('usage.total')} {formatNumber(session.total_input_tokens + session.total_output_tokens)} {t('usage.tokens')}
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
            <div className="text-center text-gray-500 py-8">{t('usage.noData')}</div>
          ) : (
            recentLogs.map((log) => (
              <div key={log.id} className="bg-gray-50 rounded-lg p-3 text-sm">
                <div className="flex justify-between">
                  <span className="font-medium">{log.model || 'unknown'}</span>
                  <span className="text-gray-500">{formatDate(log.timestamp)}</span>
                </div>
                <div className="text-gray-600 mt-1">
                  {t('usage.input')}: {formatNumber(log.input_tokens)} / {t('usage.output')}: {formatNumber(log.output_tokens)}
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
