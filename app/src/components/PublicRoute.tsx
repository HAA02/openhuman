import { Navigate } from 'react-router-dom';

import { useCoreState } from '../providers/CoreStateProvider';
import RouteLoadingScreen from './RouteLoadingScreen';

// HARDCODED dev bypass — skips Welcome/OAuth gate for closed-network testing.
// Set back to false to restore the real auth flow.
const OFFLINE_BYPASS = true;

interface PublicRouteProps {
  children: React.ReactNode;
  redirectTo?: string;
}

/**
 * Public route component that redirects authenticated users to /home.
 * Home handles the onboarding redirect once the user profile is loaded.
 */
const PublicRoute = ({ children, redirectTo }: PublicRouteProps) => {
  const { isBootstrapping, snapshot } = useCoreState();

  if (isBootstrapping) {
    return <RouteLoadingScreen />;
  }

  // If user is logged in, always go to home.
  // Home itself will redirect to onboarding if needed.
  // OFFLINE_BYPASS: skip the Welcome/OAuth gate entirely (closed-network dev).
  if (snapshot.sessionToken || OFFLINE_BYPASS) {
    return <Navigate to={redirectTo || '/home'} replace />;
  }

  // User is not logged in, show public route
  return <>{children}</>;
};

export default PublicRoute;
