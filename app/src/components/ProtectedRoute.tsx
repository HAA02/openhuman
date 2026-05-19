import { Navigate } from 'react-router-dom';

import { useCoreState } from '../providers/CoreStateProvider';
import RouteLoadingScreen from './RouteLoadingScreen';

// HARDCODED dev bypass — treats every protected route as authorized so the
// app is reachable in closed-network testing without completing OAuth.
const OFFLINE_BYPASS = true;

interface ProtectedRouteProps {
  children: React.ReactNode;
  requireAuth?: boolean;
  redirectTo?: string;
}

/**
 * Protected route component that handles authentication checks.
 * Onboarding gating is handled by the AppShell effect (see App.tsx)
 * which redirects between `/onboarding` and the rest of the app based
 * on `onboarding_completed`.
 */
const ProtectedRoute = ({ children, requireAuth = true, redirectTo }: ProtectedRouteProps) => {
  const { isBootstrapping, snapshot } = useCoreState();

  if (isBootstrapping) {
    return <RouteLoadingScreen />;
  }

  if (requireAuth && !snapshot.sessionToken && !OFFLINE_BYPASS) {
    return <Navigate to={redirectTo || '/'} replace />;
  }

  return <>{children}</>;
};

export default ProtectedRoute;
