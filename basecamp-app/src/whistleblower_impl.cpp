#include "whistleblower_impl.h"
#include <QFile>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QHttpMultiPart>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDateTime>

static const QString STORAGE_URL = QStringLiteral("http://127.0.0.1:8080");
static const QString DELIVERY_URL = QStringLiteral("http://127.0.0.1:9090");
static const QString DELIVERY_TOPIC = QStringLiteral("whistleblower/v1/documents");

WhistleblowerModule::WhistleblowerModule(QObject* parent)
    : QObject(parent) {}

void WhistleblowerModule::setStatus(const QString& s) {
    m_status = s;
    emit statusChanged();
}

void WhistleblowerModule::uploadFile(
    const QUrl& fileUrl,
    const QString& title,
    const QString& description,
    const QStringList& tags,
    bool anchorOnChain
) {
    QFile* file = new QFile(fileUrl.toLocalFile(), this);
    if (!file->open(QIODevice::ReadOnly)) {
        emit uploadFinished(QString(), false, QStringLiteral("Cannot open file: ") + fileUrl.toLocalFile());
        file->deleteLater();
        return;
    }

    m_uploading = true;
    emit uploadingChanged();
    setStatus(QStringLiteral("Uploading to Logos Storage..."));

    auto* nam = new QNetworkAccessManager(this);
    auto* multiPart = new QHttpMultiPart(QHttpMultiPart::FormDataType, this);
    QHttpPart filePart;
    filePart.setHeader(QNetworkRequest::ContentDispositionHeader,
                       QStringLiteral("form-data; name=\"file\"; filename=\"") + QFileInfo(*file).fileName() + "\"");
    filePart.setBodyDevice(file);
    file->setParent(multiPart);
    multiPart->append(filePart);

    QNetworkRequest req(QUrl(STORAGE_URL + QStringLiteral("/upload")));
    QNetworkReply* reply = nam->post(req, multiPart);
    multiPart->setParent(reply);

    connect(reply, &QNetworkReply::finished, this, [=]() {
        reply->deleteLater();
        nam->deleteLater();
        if (reply->error() != QNetworkReply::NoError) {
            m_uploading = false;
            emit uploadingChanged();
            setStatus(QStringLiteral("Upload failed: ") + reply->errorString());
            emit uploadFinished(QString(), false, reply->errorString());
            return;
        }
        QJsonDocument doc = QJsonDocument::fromJson(reply->readAll());
        QString cid = doc.object().value(QStringLiteral("cid")).toString();
        if (cid.isEmpty()) {
            m_uploading = false;
            emit uploadingChanged();
            setStatus(QStringLiteral("Upload response missing CID"));
            emit uploadFinished(QString(), false, QStringLiteral("missing CID"));
            return;
        }
        m_lastCid = cid;
        emit lastCidChanged();
        setStatus(QStringLiteral("Uploaded. CID: ") + cid + QStringLiteral(". Broadcasting..."));

        // Broadcast metadata envelope
        QJsonObject envelope;
        envelope[QStringLiteral("cid")] = cid;
        envelope[QStringLiteral("title")] = title;
        envelope[QStringLiteral("description")] = description;
        envelope[QStringLiteral("content_type")] = QStringLiteral("application/octet-stream");
        envelope[QStringLiteral("size_bytes")] = static_cast<qint64>(file->size());
        envelope[QStringLiteral("timestamp")] = QDateTime::currentSecsSinceEpoch();
        QJsonArray tagsArr;
        for (const auto& t : tags) { tagsArr.append(t); }
        envelope[QStringLiteral("tags")] = tagsArr;

        auto* nam2 = new QNetworkAccessManager(this);
        QNetworkRequest req2(QUrl(DELIVERY_URL + QStringLiteral("/publish/") + DELIVERY_TOPIC));
        req2.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/json"));
        QNetworkReply* r2 = nam2->post(req2, QJsonDocument(envelope).toJson());
        connect(r2, &QNetworkReply::finished, this, [=]() {
            r2->deleteLater();
            nam2->deleteLater();
            m_uploading = false;
            emit uploadingChanged();
            if (r2->error() != QNetworkReply::NoError) {
                setStatus(QStringLiteral("Broadcast failed: ") + r2->errorString());
                emit uploadFinished(cid, false, r2->errorString());
            } else {
                setStatus(QStringLiteral("Published. CID: ") + cid);
                emit uploadFinished(cid, true, QString());
            }
        });
    });
}

void WhistleblowerModule::runBatchAnchor() {
    setStatus(QStringLiteral("Running batch anchor... (see batch-anchor CLI for details)"));
    // Batch anchor is handled by the standalone batch-anchor CLI.
    // This method is a UI affordance; advanced users run the CLI directly.
}
