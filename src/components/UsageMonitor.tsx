import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface UsageStats {
  request_count: number;
  input_tokens: number;
  output_tokens: number;
}

export default function UsageMonitor() {
  const [stats, setStats] = useState<UsageStats>({
    request_count: 0,
    input_tokens: 0,
    output_tokens: 0,
  });

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
      const result = await invoke('get_usage_stats');
      setStats(result as UsageStats);
    } catch (error) {
      console.error('Failed to load usage stats:', error);
    }
  };

  const formatNumber = (num: number) => {
    return new Intl.NumberFormat('ko-KR').format(num);
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">사용량 모니터링</h2>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="bg-blue-50 rounded-lg p-4">
          <div className="text-sm text-blue-600 font-medium mb-1">요청 수</div>
          <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.request_count)}</div>
        </div>

        <div className="bg-green-50 rounded-lg p-4">
          <div className="text-sm text-green-600 font-medium mb-1">입력 토큰</div>
          <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.input_tokens)}</div>
        </div>

        <div className="bg-purple-50 rounded-lg p-4">
          <div className="text-sm text-purple-600 font-medium mb-1">출력 토큰</div>
          <div className="text-2xl font-bold text-gray-900">{formatNumber(stats.output_tokens)}</div>
        </div>
      </div>

      <div className="mt-4 bg-gray-50 rounded-lg p-4">
        <div className="text-sm text-gray-600 text-center">
          💡 프록시를 시작하고 Claude Code를 사용하면 실시간 사용량이 표시됩니다
        </div>
      </div>
    </div>
  );
}
