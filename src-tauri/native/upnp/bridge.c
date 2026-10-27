#include <upnp.h>
#include <upnptools.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdint.h>
#ifdef _WIN32
#include <pthread.h>
#endif

typedef void (*lap_discovery_cb)(const char *, const unsigned char *);
static lap_discovery_cb discovered;
static int client_callback(Upnp_EventType type, const void *event, void *cookie) {
    (void)cookie;
    if (type == UPNP_DISCOVERY_SEARCH_RESULT && event && discovered) {
        const UpnpDiscovery *item = event;
        const struct sockaddr_storage *address = UpnpDiscovery_get_DestAddr(item);
        if (!UpnpDiscovery_get_ErrCode(item) && address && address->ss_family == AF_INET)
            discovered(UpnpDiscovery_get_Location_cstr(item), (const unsigned char *)&((const struct sockaddr_in *)address)->sin_addr);
    }
    return 0;
}
int lap_upnp_init(lap_discovery_cb callback, const char *interface_name, int *handle) {
#ifdef _WIN32
    if (!pthread_win32_process_attach_np()) return -1;
#endif
    int result = UpnpInit2(interface_name, 0);
    if (result != UPNP_E_SUCCESS) return result;
    UpnpSetMaxContentLength(512 * 1024);
    discovered = callback;
    result = UpnpRegisterClient(client_callback, NULL, handle);
    if (result != UPNP_E_SUCCESS) UpnpFinish();
    return result;
}
const char *lap_upnp_address(void) { return UpnpGetServerIpAddress(); }
unsigned short lap_upnp_port(void) { return UpnpGetServerPort(); }
int lap_upnp_search(int handle, const char *target) { return UpnpSearchAsync(handle, 2, target, NULL); }
void lap_upnp_free(char *text) { if (text) ixmlFreeDOMString(text); }
char *lap_upnp_action_document(const char *name, const char *service, const char *const *names, const char *const *values, int count, int response) {
    IXML_Document *doc = NULL;
    for (int i = 0; i < count; i++) {
        int error = response ? UpnpAddToActionResponse(&doc, name, service, names[i], values[i]) : UpnpAddToAction(&doc, name, service, names[i], values[i]);
        if (error != UPNP_E_SUCCESS) { if (doc) ixmlDocument_free(doc); return NULL; }
    }
    if (!doc) doc = response ? UpnpMakeActionResponse(name, service, 0, NULL) : UpnpMakeAction(name, service, 0, NULL);
    if (!doc) return NULL;
    char *text = ixmlPrintDocument(doc);
    ixmlDocument_free(doc);
    return text;
}
int lap_upnp_action(int handle, const char *url, const char *service, const char *name, const char *const *names, const char *const *values, int count, char **output) {
    char *text = lap_upnp_action_document(name, service, names, values, count, 0);
    if (!text) return UPNP_E_OUTOF_MEMORY;
    IXML_Document *request = ixmlParseBuffer(text), *response = NULL;
    ixmlFreeDOMString(text);
    if (!request) return UPNP_E_INVALID_PARAM;
    int result = UpnpSendAction(handle, url, service, NULL, request, &response);
    ixmlDocument_free(request);
    *output = response ? ixmlPrintDocument(response) : NULL;
    if (response) ixmlDocument_free(response);
    return result;
}

typedef uint32_t (*lap_update_cb)(void *);
typedef struct {
    int handle;
    void *cookie;
    lap_update_cb update;
    char *protocols;
} lap_device;
static int device_callback(Upnp_EventType type, const void *event, void *cookie) {
    lap_device *device = cookie;
    if (type == UPNP_EVENT_SUBSCRIPTION_REQUEST && event) {
        const UpnpSubscriptionRequest *request = event;
        const char *service = UpnpSubscriptionRequest_get_ServiceId_cstr(request);
        const char *names[3], *values[3];
        char version[32];
        int count;
        if (strcmp(service, "urn:upnp-org:serviceId:ContentDirectory") == 0) {
            snprintf(version, sizeof(version), "%u", device->update(device->cookie));
            names[0] = "SystemUpdateID"; values[0] = version; count = 1;
        } else {
            names[0] = "SourceProtocolInfo"; values[0] = device->protocols;
            names[1] = "SinkProtocolInfo"; values[1] = "";
            names[2] = "CurrentConnectionIDs"; values[2] = "0"; count = 3;
        }
        UpnpAcceptSubscription(device->handle, UpnpSubscriptionRequest_get_UDN_cstr(request), service, names, values, count, UpnpSubscriptionRequest_get_SID_cstr(request));
    } else if (type == UPNP_CONTROL_ACTION_REQUEST && event) {
        UpnpActionRequest_set_ErrCode((UpnpActionRequest *)event, 401);
    }
    return 0;
}
int lap_upnp_publish(const char *description_url, const char *protocols, lap_update_cb update, void *cookie, void **result) {
    lap_device *device = calloc(1, sizeof(*device));
    if (!device) return UPNP_E_OUTOF_MEMORY;
    device->protocols = malloc(strlen(protocols) + 1);
    if (!device->protocols) { free(device); return UPNP_E_OUTOF_MEMORY; }
    memcpy(device->protocols, protocols, strlen(protocols) + 1);
    device->cookie = cookie; device->update = update;
    int error = UpnpRegisterRootDevice(description_url, device_callback, device, &device->handle);
    if (error == UPNP_E_SUCCESS) {
        UpnpSetMaxSubscriptions(device->handle, 16);
        UpnpSetMaxSubscriptionTimeOut(device->handle, 1800);
        error = UpnpSendAdvertisement(device->handle, 180);
        if (error != UPNP_E_SUCCESS) UpnpUnRegisterRootDevice(device->handle);
    }
    if (error != UPNP_E_SUCCESS) { free(device->protocols); free(device); return error; }
    *result = device;
    return UPNP_E_SUCCESS;
}
int lap_upnp_notify(void *opaque, const char *udn, uint32_t update) {
    lap_device *device = opaque;
    char value[32]; snprintf(value, sizeof(value), "%u", update);
    const char *names[] = { "SystemUpdateID" }, *values[] = { value };
    return UpnpNotify(device->handle, udn, "urn:upnp-org:serviceId:ContentDirectory", names, values, 1);
}
void lap_upnp_unpublish(void *opaque) {
    lap_device *device = opaque;
    if (!device) return;
    UpnpUnRegisterRootDevice(device->handle);
    free(device->protocols); free(device);
}
