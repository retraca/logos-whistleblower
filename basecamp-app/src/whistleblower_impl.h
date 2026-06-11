#pragma once
#include <QObject>
#include <QString>
#include <QUrl>

// WhistleblowerModule exposes the upload → broadcast → anchor pipeline
// to the QML Basecamp app via Q_INVOKABLE methods.
class WhistleblowerModule : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString lastCid READ lastCid NOTIFY lastCidChanged)
    Q_PROPERTY(bool uploading READ uploading NOTIFY uploadingChanged)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)

public:
    explicit WhistleblowerModule(QObject* parent = nullptr);

    [[nodiscard]] QString lastCid() const { return m_lastCid; }
    [[nodiscard]] bool uploading() const { return m_uploading; }
    [[nodiscard]] QString status() const { return m_status; }

    // Upload a file, broadcast its metadata envelope, and optionally anchor.
    Q_INVOKABLE void uploadFile(
        const QUrl& fileUrl,
        const QString& title,
        const QString& description,
        const QStringList& tags,
        bool anchorOnChain
    );

    // Run a single-batch anchor cycle for all unanchored broadcast CIDs.
    Q_INVOKABLE void runBatchAnchor();

signals:
    void lastCidChanged();
    void uploadingChanged();
    void statusChanged();
    void uploadFinished(const QString& cid, bool success, const QString& error);

private:
    QString m_lastCid;
    bool m_uploading = false;
    QString m_status;

    void setStatus(const QString& s);
};
