// Base role definitions for Blixard
// These policies implement the basic RBAC model with admin, operator, and viewer roles

// Admin Role - Full system access
permit(
    principal in Role::"admin",
    action in [
        Action::"readCluster", 
        Action::"manageCluster", 
        Action::"joinCluster", 
        Action::"leaveCluster",
        Action::"readNode", 
        Action::"manageNode",
        Action::"createVM", 
        Action::"readVM", 
        Action::"updateVM", 
        Action::"deleteVM", 
        Action::"executeVM",
        Action::"readMetrics", 
        Action::"manageBackups"
    ],
    resource
);

// Operator Role - Manage VMs and read cluster state
permit(
    principal in Role::"operator",
    action in [
        Action::"readCluster", 
        Action::"readNode",
        Action::"createVM", 
        Action::"readVM", 
        Action::"updateVM", 
        Action::"deleteVM", 
        Action::"executeVM",
        Action::"readMetrics"
    ],
    resource
);

// Viewer Role - Read-only access
permit(
    principal in Role::"viewer",
    action in [
        Action::"readCluster", 
        Action::"readNode",
        Action::"readVM", 
        Action::"readMetrics"
    ],
    resource
);